use launcher_model::{InstallationId, OperationId, ThemePreference};
use tokio::sync::broadcast;

/// How many events a slow listener may fall behind before it starts to miss
/// the oldest ones.
const EVENT_BACKLOG: usize = 256;

/// Something that happened inside the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineEvent {
	ThemeChanged(ThemePreference),
	Operation(OperationEvent),
}

/// Progress of one install, update, or removal. Terminal events are never
/// coalesced away; byte progress is throttled before it is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationEvent {
	pub operation: OperationId,
	pub installation: InstallationId,
	pub stage: Stage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
	Accepted,
	Downloading { received: u64, total: Option<u64> },
	Verifying,
	Inspecting,
	Extracting,
	Applying,
	Removing,
	Completed,
	Failed { message: String },
	RecoveryRequired { message: String },
}

impl Stage {
	pub fn is_terminal(&self) -> bool {
		matches!(
			self,
			Self::Completed | Self::Failed { .. } | Self::RecoveryRequired { .. }
		)
	}
}

/// A bounded fan-out channel. Publishing never blocks and never allocates
/// without limit.
#[derive(Clone)]
pub(crate) struct EventBus {
	sender: broadcast::Sender<EngineEvent>,
}

impl EventBus {
	pub(crate) fn new() -> Self {
		let (sender, _) = broadcast::channel(EVENT_BACKLOG);

		Self { sender }
	}

	pub(crate) fn publish(&self, event: EngineEvent) {
		// An error only means nobody is listening, which is fine.
		let _ = self.sender.send(event);
	}

	pub(crate) fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
		self.sender.subscribe()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn delivers_events_to_every_subscriber() {
		let bus = EventBus::new();
		let mut first = bus.subscribe();
		let mut second = bus.subscribe();

		bus.publish(EngineEvent::ThemeChanged(ThemePreference::Dark));

		assert_eq!(
			first.recv().await.unwrap(),
			EngineEvent::ThemeChanged(ThemePreference::Dark)
		);
		assert_eq!(
			second.recv().await.unwrap(),
			EngineEvent::ThemeChanged(ThemePreference::Dark)
		);
	}

	#[test]
	fn publishing_without_listeners_is_harmless() {
		EventBus::new().publish(EngineEvent::ThemeChanged(ThemePreference::Light));
	}
}
