# hi: शीट (इंस्टॉल की समीक्षा, संदेश, अनइंस्टॉल, डिफ़ॉल्ट बहाल करना, सभी अपडेट, नोट्स) और GitHub से जोड़ें।

sheet-ok = ठीक है
sheet-close = बंद करें

sheet-choose-title = कई पैकेज मेल खाते हैं
sheet-choose-body = एक से ज़्यादा पैकेज इस डिवाइस के लिए बराबर उपयुक्त हैं, और लॉन्चर अंदाज़ा नहीं लगाएगा कि आप कौन-सा चाहते हैं। यह कैटलॉग प्रविष्टि की कमी है, आपकी किसी गलती की वजह से नहीं।

sheet-uninstall-title = { $name } अनइंस्टॉल करें?
sheet-uninstall-installed = इंस्टॉल किया गया: { $date }
sheet-uninstall-location = कहाँ स्थित है
sheet-uninstall-what-happens = क्या होगा:
sheet-uninstall-removes = लॉन्चर द्वारा इंस्टॉल की गई फ़ाइलें हटा दी जाएँगी।
sheet-uninstall-keeps = आपने जो कुछ बदला है और ऐप ने खुद जो डेटा सहेजा है, वह रखा जाएगा।
sheet-uninstall-report = इसके बाद आप ठीक-ठीक देखेंगे कि क्या हटाया गया और क्या बचा रहा।
sheet-uninstall-confirm = अनइंस्टॉल करें

sheet-mode-intro = इसे चालू करने से पहले जान लें:
sheet-mode-risk-behavior = नया संस्करण ऐप के व्यवहार को बदल सकता है, या उसे चलने से रोक सकता है।
sheet-mode-risk-compromised = अगर किसी प्रकाशक का खाता हैक हो जाए, तो कोई हानिकारक रिलीज़ इंस्टॉल हो सकती है।
sheet-mode-risk-migration = कोई अपडेट ऐप के डेटा को इस तरह बदल सकता है कि पुराने संस्करण उसे पढ़ न सकें।
sheet-mode-risk-unattended = अपडेट बिना पूछे चलते हैं, और कोई भी स्कैन सॉफ़्टवेयर के सुरक्षित होने की गारंटी नहीं दे सकता।
sheet-mode-daemon-warning = इससे एक बैकग्राउंड प्रोग्राम भी पंजीकृत होता है, जो हर बार साइन इन करने पर शुरू होता है और लॉन्चर बंद करने के बाद भी चलता रहता है। आप इसे यहाँ या अपने सिस्टम के लॉगिन आइटम में बंद कर सकते हैं।
sheet-mode-acknowledge = मैं समझता/समझती हूँ, और मुझे स्वचालित अपडेट चाहिए
sheet-mode-confirm = चालू करें

sheet-update-all-title = सब कुछ अपडेट करें?
sheet-update-all-line = { $name }: { $from } से { $to }
sheet-update-all-note = हर ऐप को एक-एक करके जाँचा और बदला जाता है। एक में विफलता होने पर बाकी नहीं रुकते।
sheet-update-all-confirm = सभी अपडेट करें

sheet-notes-empty = इस रिलीज़ के कोई नोट्स नहीं हैं।

sheet-restore-title = डिफ़ॉल्ट बहाल करें?
sheet-restore-intro = इससे लॉन्चर की अपनी सेटिंग वैसी हो जाएँगी जैसी नई इंस्टॉलेशन में होती हैं:
sheet-restore-appearance = दिखावट फिर से आपके सिस्टम के अनुसार चलेगी।
sheet-restore-updates = अपडेट फिर से मैन्युअल हो जाएँगे, न्यूनतम आयु एक घंटे की होगी।
sheet-restore-app-settings = हर ऐप की अपनी अपडेट सेटिंग, पिन समेत, मिटा दी जाएँगी।
sheet-restore-daemon = अगर आपने बैकग्राउंड अपडेटर चालू किया था, तो वह बंद कर दिया जाएगा।
sheet-restore-note = आपके इंस्टॉल किए ऐप और उनकी फ़ाइलों को छुआ नहीं जाएगा। इसे पहले जैसा नहीं किया जा सकता।
sheet-restore-confirm = डिफ़ॉल्ट बहाल करें

sheet-removed-title = { $name } अनइंस्टॉल हो गया
sheet-removed-count = { $count ->
    [one] { $count } फ़ाइल हटाई गई।
   *[other] { $count } फ़ाइलें हटाई गईं।
}
sheet-removed-kept = कुछ फ़ाइलें रखी गईं ताकि आपने जो बनाया है वह खो न जाए:

sheet-review-install = इंस्टॉल करें
sheet-review-update = अपडेट करें
sheet-review-install-title = { $name } इंस्टॉल करें
sheet-review-update-title = { $name } अपडेट करें
sheet-review-source = स्रोत
sheet-review-unreviewed = Remade ने इस स्रोत की समीक्षा नहीं की है। सॉफ़्टवेयर केवल उन्हीं लोगों से इंस्टॉल करें जिन पर आप भरोसा करते हैं।
sheet-review-verified = सत्यापित Remade कैटलॉग में सूचीबद्ध।
sheet-review-version = संस्करण
sheet-review-file = फ़ाइल
sheet-review-checked-against = जाँच का आधार
sheet-review-skipped = नया संस्करण { $tag } छोड़ा गया: { $reason }
sheet-review-acknowledge = मैं समझता/समझती हूँ कि इस स्रोत की Remade ने समीक्षा नहीं की है
sheet-evidence-host-digest = GitHub द्वारा फ़ाइल के लिए बताया गया चेकसम। यह दिखाता है कि फ़ाइल सही-सलामत है, यह नहीं कि उसे किसने बनाया
sheet-evidence-pinned = हस्ताक्षरित Remade कैटलॉग में पिन किया गया चेकसम
sheet-evidence-signature = प्रकाशक का अपना हस्ताक्षर
sheet-evidence-local = डाउनलोड के अलावा कुछ नहीं। तुलना के लिए कोई चेकसम प्रकाशित नहीं किया गया

add-title = GitHub से जोड़ें
add-intro = किसी सार्वजनिक GitHub रिपॉज़िटरी या रिलीज़ का लिंक पेस्ट करें। इस तरह जोड़े गए ऐप की Remade ने समीक्षा नहीं की है, और चेकसम यह नहीं बता सकता कि फ़ाइल किसने बनाई या वह सुरक्षित है।
add-url-hint = https://github.com/owner/repository
add-look-up = खोजें
add-release = संस्करण { $tag }
add-partial = केवल हाल की रिलीज़ की जाँच की गई।
add-no-files = इस रिलीज़ में डाउनलोड करने योग्य ऐप फ़ाइलें नहीं हैं।
add-review-install = इंस्टॉल की समीक्षा करें
add-download-only = केवल डाउनलोड
add-fit-matches = इस डिवाइस के लिए नामित
add-fit-unknown = नाम से पता नहीं चलता कि किस डिवाइस के लिए है
add-fit-mismatch = किसी दूसरे डिवाइस के लिए नामित
