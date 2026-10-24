//! The interface's text, in the person's language.
//!
//! Text lives in Fluent files under `locales/<language>/`, one file per area
//! of the interface. Code asks for a message by id with [`t!`]:
//!
//! ```ignore
//! t!("library-empty-title")
//! t!("updates-count", count = waiting)
//! t!("settings-language.description")   // an attribute of a message
//! ```
//!
//! A message missing from the chosen language falls back to English, and one
//! missing from both shows its id, so a gap is visible rather than blank. A
//! test keeps every language to the same messages and the same variables.

use std::sync::{Arc, RwLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use launcher_model::Language;
use unic_langid::LanguageIdentifier;

/// The text of one language.
type Bundle = FluentBundle<Arc<FluentResource>>;

macro_rules! locale {
	($tag:literal) => {
		&[
			include_str!(concat!("../../locales/", $tag, "/common.ftl")),
			include_str!(concat!("../../locales/", $tag, "/explore.ftl")),
			include_str!(concat!("../../locales/", $tag, "/library.ftl")),
			include_str!(concat!("../../locales/", $tag, "/updates.ftl")),
			include_str!(concat!("../../locales/", $tag, "/settings.ftl")),
			include_str!(concat!("../../locales/", $tag, "/sheet.ftl")),
			include_str!(concat!("../../locales/", $tag, "/app.ftl")),
		]
	};
}

/// The source of each translated language.
fn sources(language: Language) -> &'static [&'static str] {
	match language {
		Language::Japanese => locale!("ja"),
		Language::PortugueseBrazil => locale!("pt-BR"),
		Language::ChineseSimplified => locale!("zh-CN"),
		Language::German => locale!("de"),
		Language::Hindi => locale!("hi"),
		Language::English | Language::Auto => locale!("en"),
	}
}

fn bundle(language: Language) -> Bundle {
	let tag = language.tag().unwrap_or("en");
	let identifier: LanguageIdentifier = tag.parse().unwrap_or_default();
	let mut bundle = FluentBundle::new_concurrent(vec![identifier]);

	// Fluent wraps inserted values in invisible direction marks. They draw as
	// empty boxes here, and none of the languages offered needs them.
	bundle.set_use_isolating(false);

	for source in sources(language) {
		match FluentResource::try_new((*source).to_owned()) {
			Ok(resource) => {
				if bundle.add_resource(Arc::new(resource)).is_err() {
					tracing::warn!(tag, "a message id is defined twice");
				}
			}
			Err((resource, errors)) => {
				tracing::warn!(tag, ?errors, "a translation file has errors");
				let _ = bundle.add_resource(Arc::new(resource));
			}
		}
	}

	bundle
}

struct Localizer {
	language: Language,
	primary: Bundle,
	/// English, for anything the chosen language lacks.
	fallback: Option<Bundle>,
}

impl Localizer {
	fn new(language: Language) -> Self {
		Self {
			language,
			primary: bundle(language),
			fallback: (language != Language::English).then(|| bundle(Language::English)),
		}
	}

	fn format(&self, id: &str, args: Option<&FluentArgs<'_>>) -> String {
		let (message, attribute) = match id.split_once('.') {
			Some((message, attribute)) => (message, Some(attribute)),
			None => (id, None),
		};

		std::iter::once(&self.primary)
			.chain(self.fallback.iter())
			.find_map(|bundle| render(bundle, message, attribute, args))
			.unwrap_or_else(|| {
				tracing::debug!(id, "no translation");

				id.to_owned()
			})
	}
}

fn render(
	bundle: &Bundle,
	message: &str,
	attribute: Option<&str>,
	args: Option<&FluentArgs<'_>>,
) -> Option<String> {
	let message = bundle.get_message(message)?;
	let pattern = match attribute {
		Some(name) => message.get_attribute(name)?.value(),
		None => message.value()?,
	};
	let mut errors = Vec::new();
	let text = bundle.format_pattern(pattern, args, &mut errors);

	if !errors.is_empty() {
		tracing::debug!(?errors, "a message could not be filled in");
	}

	Some(text.into_owned())
}

static CURRENT: RwLock<Option<Arc<Localizer>>> = RwLock::new(None);

/// Uses `language` for all text from now on. `Auto` must already have been
/// resolved by the caller.
pub(crate) fn set_language(language: Language) {
	if current_language() == language {
		return;
	}

	let localizer = Arc::new(Localizer::new(language));

	if let Ok(mut current) = CURRENT.write() {
		*current = Some(localizer);
	}
}

/// The language text is in now.
pub(crate) fn current_language() -> Language {
	CURRENT
		.read()
		.ok()
		.and_then(|current| current.as_ref().map(|localizer| localizer.language))
		.unwrap_or(Language::English)
}

fn localizer() -> Arc<Localizer> {
	if let Ok(current) = CURRENT.read()
		&& let Some(localizer) = current.as_ref()
	{
		return Arc::clone(localizer);
	}

	let localizer = Arc::new(Localizer::new(Language::English));

	if let Ok(mut current) = CURRENT.write() {
		*current = Some(Arc::clone(&localizer));
	}

	localizer
}

/// A message with nothing to fill in.
pub(crate) fn text(id: &str) -> String {
	localizer().format(id, None)
}

/// A message with values to fill in.
pub(crate) fn text_with(id: &str, args: &FluentArgs<'_>) -> String {
	localizer().format(id, Some(args))
}

/// A message by a name built at run time, such as one per setting value.
/// Every such name must exist in the files; the completeness test lists them.
pub(crate) fn text_for(prefix: &str, value: &str) -> String {
	text(&format!("{prefix}-{value}"))
}

/// Text in the person's language: `t!("id")` or `t!("id", name = value)`.
macro_rules! t {
	($id:expr) => {
		$crate::ui::i18n::text($id)
	};
	($id:expr, $($name:ident = $value:expr),+ $(,)?) => {{
		let mut args = fluent_bundle::FluentArgs::new();

		$(args.set(stringify!($name), $value);)+

		$crate::ui::i18n::text_with($id, &args)
	}};
}

#[cfg(test)]
mod tests {
	use std::collections::{BTreeMap, BTreeSet};

	use fluent_syntax::ast::{
		CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
	};
	use fluent_syntax::parser;

	use super::*;

	/// Every message in a language: its id, the variables it uses, and the
	/// attributes it has.
	type Shape = BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>;

	fn variables_in_pattern(pattern: &Pattern<&str>, found: &mut BTreeSet<String>) {
		for element in &pattern.elements {
			if let PatternElement::Placeable { expression } = element {
				variables_in_expression(expression, found);
			}
		}
	}

	fn variables_in_arguments(arguments: &CallArguments<&str>, found: &mut BTreeSet<String>) {
		for inline in arguments
			.positional
			.iter()
			.chain(arguments.named.iter().map(|named| &named.value))
		{
			variables_in_inline(inline, found);
		}
	}

	fn variables_in_inline(inline: &InlineExpression<&str>, found: &mut BTreeSet<String>) {
		match inline {
			InlineExpression::VariableReference { id } => {
				found.insert(id.name.to_owned());
			}
			InlineExpression::FunctionReference { arguments, .. } => {
				variables_in_arguments(arguments, found);
			}
			InlineExpression::TermReference {
				arguments: Some(arguments),
				..
			} => variables_in_arguments(arguments, found),
			InlineExpression::Placeable { expression } => {
				variables_in_expression(expression, found);
			}
			_ => {}
		}
	}

	fn variables_in_expression(expression: &Expression<&str>, found: &mut BTreeSet<String>) {
		match expression {
			Expression::Inline(inline) => variables_in_inline(inline, found),
			Expression::Select { selector, variants } => {
				variables_in_inline(selector, found);

				for variant in variants {
					variables_in_pattern(&variant.value, found);
				}
			}
		}
	}

	fn shape(source: &str) -> Shape {
		let resource = parser::parse(source)
			.unwrap_or_else(|(_, errors)| panic!("a translation file does not parse: {errors:?}"));
		let mut shape = Shape::new();

		for entry in &resource.body {
			if let Entry::Message(message) = entry {
				let mut variables = BTreeSet::new();
				let mut attributes = BTreeSet::new();

				if let Some(value) = &message.value {
					variables_in_pattern(value, &mut variables);
				}

				for attribute in &message.attributes {
					attributes.insert(attribute.id.name.to_owned());
					variables_in_pattern(&attribute.value, &mut variables);
				}

				assert!(
					shape
						.insert(message.id.name.to_owned(), (variables, attributes))
						.is_none(),
					"{} is defined twice",
					message.id.name
				);
			}
		}

		shape
	}

	fn shape_of(language: Language) -> Shape {
		let mut combined = Shape::new();

		for source in sources(language) {
			for (id, entry) in shape(source) {
				assert!(
					combined.insert(id.clone(), entry).is_none(),
					"{id} is defined in two files of {language:?}"
				);
			}
		}

		combined
	}

	#[test]
	fn every_language_has_the_same_messages_with_the_same_variables() {
		let english = shape_of(Language::English);

		assert!(!english.is_empty(), "English has no text");

		for language in Language::TRANSLATED {
			let other = shape_of(language);

			let missing: Vec<_> = english
				.keys()
				.filter(|id| !other.contains_key(*id))
				.collect();
			let extra: Vec<_> = other
				.keys()
				.filter(|id| !english.contains_key(*id))
				.collect();

			assert!(missing.is_empty(), "{language:?} lacks {missing:?}");
			assert!(extra.is_empty(), "{language:?} has extra {extra:?}");

			for (id, expected) in &english {
				assert_eq!(
					&other[id], expected,
					"{language:?} changes the variables or attributes of {id}"
				);
			}
		}
	}

	#[test]
	fn no_translation_is_left_as_english_by_accident() {
		// A translated message identical to English is allowed (a name, a
		// unit), but not a whole language of them.
		let english = shape_of(Language::English);

		for language in Language::TRANSLATED {
			if language == Language::English {
				continue;
			}

			let english_bundle = bundle(Language::English);
			let other = bundle(language);
			let mut same = 0;
			let mut total = 0;

			for id in english.keys() {
				let (Some(a), Some(b)) = (
					english_bundle.get_message(id).and_then(|m| m.value()),
					other.get_message(id).and_then(|m| m.value()),
				) else {
					continue;
				};
				let mut errors = Vec::new();

				total += 1;

				if english_bundle.format_pattern(a, None, &mut errors)
					== other.format_pattern(b, None, &mut errors)
				{
					same += 1;
				}
			}

			assert!(
				same * 4 <= total,
				"{language:?} leaves {same} of {total} messages in English"
			);
		}
	}

	#[test]
	fn text_falls_back_to_english_and_then_to_the_id() {
		let localizer = Localizer::new(Language::German);

		assert_eq!(localizer.format("no-such-message", None), "no-such-message");
	}

	#[test]
	fn values_are_filled_in_without_hidden_direction_marks() {
		let english = bundle(Language::English);
		let mut args = FluentArgs::new();

		args.set("name", "Quill");

		let mut errors = Vec::new();
		let source = FluentResource::try_new("hello = Hello, { $name }!".to_owned()).unwrap();
		let mut test = FluentBundle::new_concurrent(vec!["en".parse().unwrap()]);

		test.set_use_isolating(false);
		test.add_resource(Arc::new(source)).unwrap();

		let message = test.get_message("hello").unwrap();
		let text = test.format_pattern(message.value().unwrap(), Some(&args), &mut errors);

		assert_eq!(text, "Hello, Quill!");
		assert!(english.get_message("no-such").is_none());
	}
}
