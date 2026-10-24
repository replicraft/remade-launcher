# सेटिंग: भाषा और क्षेत्र

settings-language = भाषा
    .description = इंटरफ़ेस की भाषा। “स्वचालित” आपके सिस्टम की भाषा ({ $system }) का पालन करता है।
settings-language-auto = स्वचालित
settings-time-format = समय का प्रारूप
    .description = “सिस्टम” चुनने पर आपके सिस्टम के क्षेत्र की सेटिंग लागू होती है।
time-format-system = सिस्टम
time-format-12h = 12 घंटे (3:42 PM)
time-format-24h = 24 घंटे (15:42)
settings-date-format = तारीख़ का प्रारूप
    .description = “सिस्टम” चुनने पर आपके सिस्टम के क्षेत्र की सेटिंग लागू होती है।
date-format-system = सिस्टम
date-format-mdy = अक्टू॰ 24, 2026
date-format-dmy = 24 अक्टू॰ 2026
date-format-iso = 2026-10-24
settings-region-example = उदाहरण: { $today }   ·   { $earlier }

# सेटिंग: पैनल और बाकी सब।

settings-title = सेटिंग
settings-appearance = रूप-रंग
settings-region-panel = भाषा और क्षेत्र
settings-updates-panel = अपडेट
    .description = इंस्टॉल किए गए ऐप कैसे अपडेट होते हैं। लॉन्चर अपने आप को नीचे अलग से अपडेट करता है।
settings-launcher-panel = लॉन्चर अपडेट
    .description = लॉन्चर खुद को कैसे अप-टू-डेट रखता है। इससे आपके ऐप के अपडेट होने का तरीका नहीं बदलता।
settings-notifications-panel = सूचनाएँ
settings-network-panel = नेटवर्क
settings-storage-panel = स्टोरेज
settings-support-panel = सहायता
settings-about-panel = परिचय

settings-theme = थीम
theme-system = सिस्टम
theme-light = हल्की
theme-dark = गहरी
settings-scale = इंटरफ़ेस का आकार
    .description = सब कुछ बड़ा या छोटा करता है। हाई-रेज़ोल्यूशन स्क्रीन पर काम आता है।
settings-start-page = शुरू में खोलें
    .description = लॉन्चर शुरू होने पर जो पेज दिखता है।
start-page-auto = ऐप हों तो लाइब्रेरी, वरना खोजें
start-page-library = लाइब्रेरी
start-page-explore = खोजें
start-page-last-used = जहाँ छोड़ा था
settings-layout-note = आपका लेआउट, जैसे सिमटा हुआ साइडबार, ग्रिड या सूची, और विंडो का आकार अपने आप याद रखा जाता है।

update-mode-pinned = पिन किया हुआ
    .description = जो संस्करण है, उसी पर रहता है। जब तक आप कोई और स्तर नहीं चुनते, कुछ भी इसे अपडेट नहीं करता और अपडेट बटन भी नहीं दिखता।
update-mode-manual = मैन्युअल अपडेट (डिफ़ॉल्ट)
    .description = लॉन्चर खोलने पर अपडेट जाँचता है। इंस्टॉल किए गए ऐप पर अपडेट बटन दिखता है, इसलिए इंस्टॉल कब करना है यह आप तय करते हैं।
update-mode-on-launch = लॉन्चर शुरू होने पर अपने आप अपडेट करें
    .description = हर बार लॉन्चर शुरू करने पर योग्य ऐप अपडेट अपने आप इंस्टॉल होते हैं। लॉन्चर बंद करने के बाद यह चलता नहीं रहता और बैकग्राउंड में ऐप अपडेट नहीं करता।
update-mode-daemon = हमेशा सक्रिय बैकग्राउंड अपडेटर से अपने आप अपडेट करें (खतरनाक)
    .description = साइन इन करने पर एक हल्का बैकग्राउंड अपडेटर शुरू होता है। यह शुरू होने पर और सक्रिय रहने तक हर 24 घंटे में जाँच करता है, और लॉन्चर खोलने पर भी। अपडेट अलग-अलग पुष्टि के बिना इंस्टॉल होते हैं। दुर्भावनापूर्ण, समझौता किए गए या खराब अपस्ट्रीम रिलीज़ अपने आप इंस्टॉल हो सकते हैं और सॉफ़्टवेयर या डेटा को नुकसान पहुँचा सकते हैं।
settings-min-age = ऑटो-अपडेट के लिए डिफ़ॉल्ट न्यूनतम रिलीज़ आयु
    .automatic = ऑटोमैटिक अपडेट किसी रिलीज़ को इंस्टॉल करे, इसके लिए वह कम से कम इतनी पुरानी होनी चाहिए। हर ऐप अपनी अलग सीमा रख सकता है।
    .manual = केवल ऑटोमैटिक मोड में इस्तेमाल होती है। आपके खुद शुरू किए अपडेट को यह कभी नहीं रोकती।
minimum-age-0 = कोई देरी नहीं
minimum-age-3600 = 1 घंटा
minimum-age-21600 = 6 घंटे
minimum-age-43200 = 12 घंटे
minimum-age-86400 = 24 घंटे
minimum-age-259200 = 3 दिन
minimum-age-604800 = 7 दिन
settings-min-age-warning = देरी न होने पर रिलीज़ प्रकाशित होते ही इंस्टॉल हो जाती है। इसमें जोखिम ज़्यादा है।
settings-background = बैकग्राउंड अपडेटर
settings-background-checking = जाँच रहे हैं…
settings-background-active = सक्रिय
settings-background-stopped = रुका हुआ
    .detail = आपके साइन इन करने पर यह शुरू होगा।
settings-background-pending = OS की मंज़ूरी बाकी
    .detail = अपने सिस्टम के लॉगिन आइटम में इसे मंज़ूर करें।
settings-background-disabled = OS ने बंद किया
    .detail = आपने इसे सिस्टम सेटिंग में बंद किया है। लॉन्चर इसे दोबारा चालू नहीं करेगा।
settings-background-not-installed = इंस्टॉल नहीं है
settings-background-failed = विफल
settings-background-unavailable = यहाँ उपलब्ध नहीं
settings-background-last-check = पिछली जाँच: { $when }
settings-background-last-result = पिछला नतीजा: { $outcome }
settings-background-next-check = अगली जाँच: { $when }
settings-background-stop = बैकग्राउंड अपडेट रोकें

launcher-updates-automatic = अपने आप
    .description = नया लॉन्चर बैकग्राउंड में डाउनलोड करता है और इसे बंद करने पर उस पर स्विच कर देता है।
launcher-updates-notify = मुझे बताएँ
    .description = नया लॉन्चर उपलब्ध होने पर बताता है, बाकी आप पर छोड़ता है।
launcher-updates-off = बंद
    .description = नया लॉन्चर कभी नहीं खोजता।
settings-launcher-last-checked = पिछली जाँच { $when }
settings-launcher-staged = { $version } डाउनलोड हो गया है। यह अगली बार लॉन्चर खोलने पर शुरू होगा।
settings-launcher-available = { $version } उपलब्ध है।
settings-launcher-applied = { $when } को { $from } से { $to } पर स्विच किया गया।
settings-launcher-checking = जाँच रहे हैं…
settings-launcher-check-now = अभी जाँचें
settings-launcher-download = अभी डाउनलोड करें
settings-launcher-roll-back = पिछले संस्करण पर लौटें

settings-notify-updates = अपडेट उपलब्ध होने पर मुझे बताएँ
settings-notify-finished = इंस्टॉल या अपडेट पूरा होने पर मुझे बताएँ
settings-notify-note = सूचनाएँ केवल तब दिखती हैं जब लॉन्चर बैकग्राउंड में हो। आपका सिस्टम पहली बार अनुमति माँग सकता है।
settings-notify-test = टेस्ट सूचना भेजें

settings-proxy = प्रॉक्सी
    .description = ऑटोमैटिक में आपके सिस्टम की प्रॉक्सी सेटिंग इस्तेमाल होती हैं, सामान्य एनवायरनमेंट वेरिएबल समेत।
settings-proxy-auto = ऑटोमैटिक (सिस्टम)
settings-proxy-none = कोई प्रॉक्सी नहीं
settings-proxy-custom = कस्टम पता
settings-proxy-hint = http://, https:// या socks5://। यूज़र नेम और पासवर्ड न डालें: वे बिना सुरक्षा के सेव होंगे।
settings-proxy-use = यह प्रॉक्सी इस्तेमाल करें
settings-proxy-problem-empty = कोई पता डालें, जैसे http://proxy.example:8080।
settings-proxy-problem-address = यह मान्य पता नहीं है।
settings-proxy-problem-scheme = http://, https:// या socks5:// पते का इस्तेमाल करें।
settings-proxy-problem-credentials = यूज़र नेम और पासवर्ड न डालें। वे बिना सुरक्षा के सेव होंगे।
settings-timeout = रिक्वेस्ट टाइमआउट
    .description = हार मानने से पहले सर्वर का कितना इंतज़ार करना है। धीमे कनेक्शन को ज़्यादा समय चाहिए हो सकता है।
settings-seconds = { $count ->
    [one] { $count } सेकंड
   *[other] { $count } सेकंड
}
settings-github = GitHub एक्सेस
settings-github-intro = GitHub हर किसी को प्रति घंटे 60 लुकअप देता है, और मुफ़्त टोकन के साथ 5,000। लॉन्चर पहले पूछी बातें याद रखता है, इसलिए ज़्यादातर जाँचों में कोई गिनती नहीं लगती। टोकन तभी जोड़ें जब आपके पास बहुत सारे ऐप हों और फिर भी सीमा पूरी हो जाए। बिना किसी अनुमति वाला टोकन काफ़ी है, क्योंकि लॉन्चर जो कुछ पढ़ता है वह सार्वजनिक है।
settings-token-stored = एक टोकन आपके सिस्टम के क्रेडेंशियल स्टोर में सेव है।
settings-token-remove = टोकन हटाएँ
settings-token-environment = एक टोकन आपके एनवायरनमेंट की { $name } सेटिंग से आ रहा है।
settings-token-unavailable = आपके सिस्टम का क्रेडेंशियल स्टोर उपलब्ध नहीं है, इसलिए टोकन सुरक्षित रूप से नहीं रखा जा सकता।
settings-token-paste = टोकन पेस्ट करें
settings-token-save = टोकन सेव करें
settings-rate-check = मेरी सीमा देखें
settings-rate-left = { $limit } में से { $remaining } लुकअप बचे हैं। रीसेट { $resets }। { $note }
settings-rate-token = आपका टोकन इस्तेमाल हो रहा है।
settings-rate-anonymous = साइन इन नहीं है।

settings-measuring = माप रहे हैं…
settings-show-folder = फ़ाइल मैनेजर में दिखाएँ
settings-storage-apps = आपके ऐप
    .description = लॉन्चर ने जो कुछ इंस्टॉल किया है। जगह खाली करने के लिए कोई ऐप अनइंस्टॉल करें।
settings-storage-leftover = बचे हुए डाउनलोड
    .description = अधूरे रह गए इंस्टॉल की फ़ाइलें। इन्हें हटाना सुरक्षित है; चल रही किसी चीज़ को नहीं छुआ जाता।
settings-clear-leftovers = बचा हुआ साफ़ करें
settings-storage-cache = याद रखे गए लुकअप
    .description = GitHub के जवाब, जो जाँच को तेज़ और सस्ता बनाने के लिए रखे जाते हैं। ज़रूरत पड़ने पर ये दोबारा भर जाते हैं।
settings-clear-cache = याद रखे गए लुकअप साफ़ करें
settings-storage-data = लॉन्चर डेटा
    .description = लॉन्चर के अपने रिकॉर्ड: क्या इंस्टॉल है, सेटिंग और इतिहास।

settings-support-copy = सहायता रिपोर्ट कॉपी करें
settings-support-note = रिपोर्ट में केवल संस्करण और गिनतियाँ होती हैं। इसमें फ़ाइल पाथ, लिंक या ऐप के नाम नहीं होते।

settings-about-by = Replicraft का { $name }
settings-about-version = संस्करण { $version }
settings-about-platform = प्लैटफ़ॉर्म
settings-about-license = लाइसेंस
settings-about-website = वेबसाइट
settings-about-source = GitHub पर सोर्स देखें
settings-about-restore = डिफ़ॉल्ट बहाल करें
