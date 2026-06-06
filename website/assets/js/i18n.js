'use strict';

/* ================================================
   HIMSHA Network — lightweight i18n (progressive enhancement)

   Dictionaries are EMBEDDED here (no fetch) so the language switcher
   works whether the page is opened over http(s) OR as a local file://
   URL — browsers block fetch() of local JSON on file://.
   The same data also lives in assets/i18n/*.json for tooling reference.
================================================ */
(function () {
  const DICTS = {
    en: {
      "nav.why":"Why HIMSHA","nav.how":"How It Works","nav.programs":"Programs","nav.usecases":"Use Cases","nav.sdks":"SDKs","nav.tutorials":"Tutorials","nav.getStarted":"Get Started",
      "hero.badge":"Educational · Fully open source","hero.title":"Bitcoin programmability, proven by zero knowledge.","hero.sub":"HIMSHA runs programs inside a ZK prover. Every state transition ships with a cryptographic receipt — not a validator vote. Build on Bitcoin with no bridges and no trust assumptions. Every layer is fully open source.","hero.start":"Start Building","hero.source":"View Source",
      "stats.programs":"Programs","stats.sdks":"SDKs","stats.bridges":"Bridges","stats.proven":"Proven",
      "why.title":"Math, not majority vote","how.title":"How HIMSHA works","programs.title":"Built-in programs","usecases.title":"What you can build","sdks.title":"Build in your language","start.title":"Up and running in 3 steps","docs.title":"Everything you need",
      "faq.label":"FAQ","faq.title":"Frequently asked questions","ask.title":"Have a question?","ask.qLabel":"Question","ask.dLabel":"Details","ask.discuss":"Ask in Discussions","ask.issue":"Open as an issue",
      "cta.title":"Start proving on Bitcoin","cta.start":"Get Started","cta.tutorials":"Read the Tutorials",
      "meta.title":"HIMSHA Network — ZK-Proven Bitcoin Programmability","meta.desc":"HIMSHA Network is a fully open-source, ZK-proven Bitcoin programmability layer. Every state transition is proven correct by a cryptographic receipt — not validator majority. No bridges, no trust assumptions."
    },
    es: {
      "nav.why":"Por qué HIMSHA","nav.how":"Cómo funciona","nav.programs":"Programas","nav.usecases":"Casos de uso","nav.sdks":"SDKs","nav.tutorials":"Tutoriales","nav.getStarted":"Empezar",
      "hero.badge":"Educativo · Código totalmente abierto","hero.title":"Programabilidad de Bitcoin, probada con conocimiento cero.","hero.sub":"HIMSHA ejecuta programas dentro de un probador ZK. Cada transición de estado incluye un recibo criptográfico, no un voto de validadores. Construye sobre Bitcoin sin puentes ni suposiciones de confianza. Cada capa es de código totalmente abierto.","hero.start":"Empezar a construir","hero.source":"Ver código",
      "stats.programs":"Programas","stats.sdks":"SDKs","stats.bridges":"Puentes","stats.proven":"Probado",
      "why.title":"Matemáticas, no voto mayoritario","how.title":"Cómo funciona HIMSHA","programs.title":"Programas integrados","usecases.title":"Qué puedes construir","sdks.title":"Construye en tu lenguaje","start.title":"En marcha en 3 pasos","docs.title":"Todo lo que necesitas",
      "faq.label":"Preguntas frecuentes","faq.title":"Preguntas frecuentes","ask.title":"¿Tienes una pregunta?","ask.qLabel":"Pregunta","ask.dLabel":"Detalles","ask.discuss":"Preguntar en Discussions","ask.issue":"Abrir una incidencia",
      "cta.title":"Empieza a probar en Bitcoin","cta.start":"Empezar","cta.tutorials":"Leer los tutoriales",
      "meta.title":"HIMSHA Network — Programabilidad de Bitcoin probada con ZK","meta.desc":"HIMSHA Network es una capa de programabilidad de Bitcoin de código totalmente abierto y probada con ZK. Cada transición de estado se prueba correcta mediante un recibo criptográfico, no por mayoría de validadores."
    },
    fr: {
      "nav.why":"Pourquoi HIMSHA","nav.how":"Comment ça marche","nav.programs":"Programmes","nav.usecases":"Cas d'usage","nav.sdks":"SDK","nav.tutorials":"Tutoriels","nav.getStarted":"Commencer",
      "hero.badge":"Éducatif · Entièrement open source","hero.title":"La programmabilité de Bitcoin, prouvée par la connaissance zéro.","hero.sub":"HIMSHA exécute des programmes dans un prouveur ZK. Chaque transition d'état est accompagnée d'un reçu cryptographique, et non d'un vote de validateurs. Construisez sur Bitcoin sans ponts ni hypothèses de confiance. Chaque couche est entièrement open source.","hero.start":"Commencer à construire","hero.source":"Voir le code",
      "stats.programs":"Programmes","stats.sdks":"SDK","stats.bridges":"Ponts","stats.proven":"Prouvé",
      "why.title":"Les maths, pas le vote majoritaire","how.title":"Comment fonctionne HIMSHA","programs.title":"Programmes intégrés","usecases.title":"Ce que vous pouvez construire","sdks.title":"Développez dans votre langage","start.title":"Opérationnel en 3 étapes","docs.title":"Tout ce qu'il vous faut",
      "faq.label":"FAQ","faq.title":"Questions fréquentes","ask.title":"Une question ?","ask.qLabel":"Question","ask.dLabel":"Détails","ask.discuss":"Poser dans Discussions","ask.issue":"Ouvrir un ticket",
      "cta.title":"Commencez à prouver sur Bitcoin","cta.start":"Commencer","cta.tutorials":"Lire les tutoriels",
      "meta.title":"HIMSHA Network — Programmabilité Bitcoin prouvée par ZK","meta.desc":"HIMSHA Network est une couche de programmabilité Bitcoin entièrement open source et prouvée par ZK. Chaque transition d'état est prouvée correcte par un reçu cryptographique, et non par une majorité de validateurs."
    },
    de: {
      "nav.why":"Warum HIMSHA","nav.how":"So funktioniert es","nav.programs":"Programme","nav.usecases":"Anwendungsfälle","nav.sdks":"SDKs","nav.tutorials":"Tutorials","nav.getStarted":"Loslegen",
      "hero.badge":"Bildungsprojekt · Vollständig quelloffen","hero.title":"Bitcoin-Programmierbarkeit, bewiesen durch Zero Knowledge.","hero.sub":"HIMSHA führt Programme in einem ZK-Prover aus. Jeder Zustandsübergang kommt mit einem kryptografischen Beleg – nicht mit einem Validator-Votum. Baue auf Bitcoin ohne Bridges und ohne Vertrauensannahmen. Jede Schicht ist vollständig quelloffen.","hero.start":"Jetzt entwickeln","hero.source":"Quellcode ansehen",
      "stats.programs":"Programme","stats.sdks":"SDKs","stats.bridges":"Bridges","stats.proven":"Bewiesen",
      "why.title":"Mathematik statt Mehrheitsentscheid","how.title":"So funktioniert HIMSHA","programs.title":"Integrierte Programme","usecases.title":"Was du bauen kannst","sdks.title":"Entwickle in deiner Sprache","start.title":"In 3 Schritten startklar","docs.title":"Alles, was du brauchst",
      "faq.label":"FAQ","faq.title":"Häufige Fragen","ask.title":"Hast du eine Frage?","ask.qLabel":"Frage","ask.dLabel":"Details","ask.discuss":"In Discussions fragen","ask.issue":"Issue öffnen",
      "cta.title":"Beginne, auf Bitcoin zu beweisen","cta.start":"Loslegen","cta.tutorials":"Tutorials lesen",
      "meta.title":"HIMSHA Network — ZK-bewiesene Bitcoin-Programmierbarkeit","meta.desc":"HIMSHA Network ist eine vollständig quelloffene, ZK-bewiesene Bitcoin-Programmierbarkeitsschicht. Jeder Zustandsübergang wird durch einen kryptografischen Beleg als korrekt bewiesen – nicht durch Validator-Mehrheit."
    },
    "zh-Hans": {
      "nav.why":"为何选择 HIMSHA","nav.how":"工作原理","nav.programs":"程序","nav.usecases":"应用场景","nav.sdks":"SDK","nav.tutorials":"教程","nav.getStarted":"开始使用",
      "hero.badge":"教育项目 · 完全开源","hero.title":"比特币可编程性，由零知识证明。","hero.sub":"HIMSHA 在 ZK 证明器中运行程序。每次状态转换都附带密码学回执，而非验证者投票。在比特币上构建，无需跨链桥，无信任假设。每一层都完全开源。","hero.start":"开始构建","hero.source":"查看源码",
      "stats.programs":"程序","stats.sdks":"SDK","stats.bridges":"跨链桥","stats.proven":"已证明",
      "why.title":"靠数学，而非多数投票","how.title":"HIMSHA 的工作原理","programs.title":"内置程序","usecases.title":"你能构建什么","sdks.title":"用你的语言构建","start.title":"三步即可运行","docs.title":"你所需要的一切",
      "faq.label":"常见问题","faq.title":"常见问题","ask.title":"有问题吗？","ask.qLabel":"问题","ask.dLabel":"详情","ask.discuss":"在 Discussions 提问","ask.issue":"提交 issue",
      "cta.title":"开始在比特币上证明","cta.start":"开始使用","cta.tutorials":"阅读教程",
      "meta.title":"HIMSHA Network — 由 ZK 证明的比特币可编程性","meta.desc":"HIMSHA Network 是一个完全开源、由零知识证明保障的比特币可编程性层。每一次状态转换都由密码学回执证明其正确性，而非验证者多数投票。无需跨链桥，无信任假设。"
    },
    ja: {
      "nav.why":"HIMSHAとは","nav.how":"仕組み","nav.programs":"プログラム","nav.usecases":"ユースケース","nav.sdks":"SDK","nav.tutorials":"チュートリアル","nav.getStarted":"始める",
      "hero.badge":"教育目的 · 完全オープンソース","hero.title":"ゼロ知識で証明される、Bitcoinのプログラマビリティ。","hero.sub":"HIMSHAはZKプルーバー内でプログラムを実行します。すべての状態遷移には、バリデータの投票ではなく暗号学的レシートが付随します。ブリッジや信頼の前提なしにBitcoin上で構築できます。すべての層が完全オープンソースです。","hero.start":"開発を始める","hero.source":"ソースを見る",
      "stats.programs":"プログラム","stats.sdks":"SDK","stats.bridges":"ブリッジ","stats.proven":"証明済み",
      "why.title":"多数決ではなく、数学で","how.title":"HIMSHAの仕組み","programs.title":"組み込みプログラム","usecases.title":"作れるもの","sdks.title":"好きな言語で開発","start.title":"3ステップで起動","docs.title":"必要なものすべて",
      "faq.label":"よくある質問","faq.title":"よくある質問","ask.title":"質問がありますか？","ask.qLabel":"質問","ask.dLabel":"詳細","ask.discuss":"Discussionsで質問","ask.issue":"Issueを作成",
      "cta.title":"Bitcoin上で証明を始めよう","cta.start":"始める","cta.tutorials":"チュートリアルを読む",
      "meta.title":"HIMSHA Network — ZKで証明されるBitcoinのプログラマビリティ","meta.desc":"HIMSHA Networkは、完全オープンソースでZK証明されたBitcoinプログラマビリティ層です。すべての状態遷移は、バリデータの多数決ではなく暗号学的レシートによって正しさが証明されます。ブリッジ不要、信頼の前提なし。"
    },
    hi: {
      "nav.why":"HIMSHA क्यों","nav.how":"यह कैसे काम करता है","nav.programs":"प्रोग्राम","nav.usecases":"उपयोग के मामले","nav.sdks":"SDK","nav.tutorials":"ट्यूटोरियल","nav.getStarted":"शुरू करें",
      "hero.badge":"शैक्षिक · पूर्णतः ओपन सोर्स","hero.title":"बिटकॉइन प्रोग्रामेबिलिटी, ज़ीरो नॉलेज से सिद्ध।","hero.sub":"HIMSHA प्रोग्रामों को एक ZK प्रूवर के भीतर चलाता है। हर स्टेट ट्रांज़िशन के साथ एक क्रिप्टोग्राफ़िक रसीद आती है — वैलिडेटर वोट नहीं। बिना ब्रिज और बिना भरोसे की धारणाओं के बिटकॉइन पर बनाएं। हर परत पूर्णतः ओपन सोर्स है।","hero.start":"बनाना शुरू करें","hero.source":"सोर्स देखें",
      "stats.programs":"प्रोग्राम","stats.sdks":"SDK","stats.bridges":"ब्रिज","stats.proven":"सिद्ध",
      "why.title":"गणित, बहुमत वोट नहीं","how.title":"HIMSHA कैसे काम करता है","programs.title":"अंतर्निहित प्रोग्राम","usecases.title":"आप क्या बना सकते हैं","sdks.title":"अपनी भाषा में बनाएं","start.title":"3 चरणों में तैयार","docs.title":"आपकी ज़रूरत की हर चीज़",
      "faq.label":"सामान्य प्रश्न","faq.title":"अक्सर पूछे जाने वाले प्रश्न","ask.title":"कोई प्रश्न है?","ask.qLabel":"प्रश्न","ask.dLabel":"विवरण","ask.discuss":"Discussions में पूछें","ask.issue":"इश्यू खोलें",
      "cta.title":"बिटकॉइन पर सिद्ध करना शुरू करें","cta.start":"शुरू करें","cta.tutorials":"ट्यूटोरियल पढ़ें",
      "meta.title":"HIMSHA Network — ZK से सिद्ध बिटकॉइन प्रोग्रामेबिलिटी","meta.desc":"HIMSHA Network एक पूर्णतः ओपन-सोर्स, ZK-सिद्ध बिटकॉइन प्रोग्रामेबिलिटी परत है। हर स्टेट ट्रांज़िशन की शुद्धता वैलिडेटर बहुमत से नहीं, बल्कि एक क्रिप्टोग्राफ़िक रसीद से सिद्ध होती है।"
    },
    te: {
      "nav.why":"ఎందుకు HIMSHA","nav.how":"ఇది ఎలా పనిచేస్తుంది","nav.programs":"ప్రోగ్రామ్‌లు","nav.usecases":"వినియోగ సందర్భాలు","nav.sdks":"SDKలు","nav.tutorials":"ట్యుటోరియల్స్","nav.getStarted":"ప్రారంభించండి",
      "hero.badge":"విద్యాపరమైనది · పూర్తిగా ఓపెన్ సోర్స్","hero.title":"బిట్‌కాయిన్ ప్రోగ్రామబిలిటీ, జీరో నాలెడ్జ్ ద్వారా నిరూపితం.","hero.sub":"HIMSHA ప్రోగ్రామ్‌లను ఒక ZK ప్రూవర్ లోపల అమలు చేస్తుంది. ప్రతి స్టేట్ ట్రాన్సిషన్‌తో వాలిడేటర్ ఓటు కాకుండా క్రిప్టోగ్రాఫిక్ రసీదు వస్తుంది. బ్రిడ్జ్‌లు, నమ్మక ఊహలు లేకుండా బిట్‌కాయిన్‌పై నిర్మించండి. ప్రతి లేయర్ పూర్తిగా ఓపెన్ సోర్స్.","hero.start":"నిర్మాణం ప్రారంభించండి","hero.source":"సోర్స్ చూడండి",
      "stats.programs":"ప్రోగ్రామ్‌లు","stats.sdks":"SDKలు","stats.bridges":"బ్రిడ్జ్‌లు","stats.proven":"నిరూపితం",
      "why.title":"గణితం, మెజారిటీ ఓటు కాదు","how.title":"HIMSHA ఎలా పనిచేస్తుంది","programs.title":"అంతర్నిర్మిత ప్రోగ్రామ్‌లు","usecases.title":"మీరు ఏమి నిర్మించవచ్చు","sdks.title":"మీ భాషలో నిర్మించండి","start.title":"3 దశల్లో సిద్ధం","docs.title":"మీకు కావలసినవన్నీ",
      "faq.label":"ప్రశ్నలు","faq.title":"తరచుగా అడిగే ప్రశ్నలు","ask.title":"ప్రశ్న ఉందా?","ask.qLabel":"ప్రశ్న","ask.dLabel":"వివరాలు","ask.discuss":"Discussionsలో అడగండి","ask.issue":"ఇష్యూ తెరవండి",
      "cta.title":"బిట్‌కాయిన్‌పై నిరూపించడం ప్రారంభించండి","cta.start":"ప్రారంభించండి","cta.tutorials":"ట్యుటోరియల్స్ చదవండి",
      "meta.title":"HIMSHA Network — ZK ద్వారా నిరూపితమైన బిట్‌కాయిన్ ప్రోగ్రామబిలిటీ","meta.desc":"HIMSHA Network పూర్తిగా ఓపెన్-సోర్స్, ZK-నిరూపిత బిట్‌కాయిన్ ప్రోగ్రామబిలిటీ లేయర్. ప్రతి స్టేట్ ట్రాన్సిషన్ సరైనదని వాలిడేటర్ మెజారిటీ ద్వారా కాకుండా క్రిప్టోగ్రాఫిక్ రసీదు ద్వారా నిరూపించబడుతుంది."
    },
    ta: {
      "nav.why":"ஏன் HIMSHA","nav.how":"இது எப்படி வேலை செய்கிறது","nav.programs":"நிரல்கள்","nav.usecases":"பயன்பாட்டு வழக்குகள்","nav.sdks":"SDKகள்","nav.tutorials":"பயிற்சிகள்","nav.getStarted":"தொடங்குங்கள்",
      "hero.badge":"கல்வி நோக்கம் · முழுமையாக திறந்த மூலம்","hero.title":"பிட்காயின் நிரல்தன்மை, ஜீரோ நாலெட்ஜ் மூலம் நிரூபிக்கப்பட்டது.","hero.sub":"HIMSHA நிரல்களை ஒரு ZK ப்ரூவருக்குள் இயக்குகிறது. ஒவ்வொரு நிலை மாற்றத்துடனும் சரிபார்ப்பாளர் வாக்கு அல்ல, ஒரு கிரிப்டோகிராஃபிக் ரசீது வருகிறது. பாலங்கள் மற்றும் நம்பிக்கை அனுமானங்கள் இல்லாமல் பிட்காயினில் உருவாக்குங்கள். ஒவ்வொரு அடுக்கும் முழுமையாக திறந்த மூலம்.","hero.start":"உருவாக்கத் தொடங்குங்கள்","hero.source":"மூலத்தைப் பார்க்கவும்",
      "stats.programs":"நிரல்கள்","stats.sdks":"SDKகள்","stats.bridges":"பாலங்கள்","stats.proven":"நிரூபிக்கப்பட்டது",
      "why.title":"கணிதம், பெரும்பான்மை வாக்கு அல்ல","how.title":"HIMSHA எப்படி வேலை செய்கிறது","programs.title":"உள்ளமைந்த நிரல்கள்","usecases.title":"நீங்கள் என்ன உருவாக்கலாம்","sdks.title":"உங்கள் மொழியில் உருவாக்குங்கள்","start.title":"3 படிகளில் தயார்","docs.title":"உங்களுக்குத் தேவையான அனைத்தும்",
      "faq.label":"கேள்விகள்","faq.title":"அடிக்கடி கேட்கப்படும் கேள்விகள்","ask.title":"கேள்வி உள்ளதா?","ask.qLabel":"கேள்வி","ask.dLabel":"விவரங்கள்","ask.discuss":"Discussions இல் கேளுங்கள்","ask.issue":"issue திறக்கவும்",
      "cta.title":"பிட்காயினில் நிரூபிக்கத் தொடங்குங்கள்","cta.start":"தொடங்குங்கள்","cta.tutorials":"பயிற்சிகளைப் படியுங்கள்",
      "meta.title":"HIMSHA Network — ZK மூலம் நிரூபிக்கப்பட்ட பிட்காயின் நிரல்தன்மை","meta.desc":"HIMSHA Network என்பது முழுமையாக திறந்த மூல, ZK-நிரூபிக்கப்பட்ட பிட்காயின் நிரல்தன்மை அடுக்கு. ஒவ்வொரு நிலை மாற்றமும் சரியானது என்பது சரிபார்ப்பாளர் பெரும்பான்மையால் அல்ல, ஒரு கிரிப்டோகிராஃபிக் ரசீதால் நிரூபிக்கப்படுகிறது."
    },
    kn: {
      "nav.why":"ಏಕೆ HIMSHA","nav.how":"ಇದು ಹೇಗೆ ಕೆಲಸ ಮಾಡುತ್ತದೆ","nav.programs":"ಪ್ರೋಗ್ರಾಂಗಳು","nav.usecases":"ಬಳಕೆಯ ಸಂದರ್ಭಗಳು","nav.sdks":"SDKಗಳು","nav.tutorials":"ಟ್ಯುಟೋರಿಯಲ್‌ಗಳು","nav.getStarted":"ಪ್ರಾರಂಭಿಸಿ",
      "hero.badge":"ಶೈಕ್ಷಣಿಕ · ಸಂಪೂರ್ಣ ಮುಕ್ತ ಮೂಲ","hero.title":"ಬಿಟ್‌ಕಾಯಿನ್ ಪ್ರೋಗ್ರಾಮೆಬಿಲಿಟಿ, ಶೂನ್ಯ ಜ್ಞಾನದಿಂದ ಸಾಬೀತು.","hero.sub":"HIMSHA ಪ್ರೋಗ್ರಾಂಗಳನ್ನು ZK ಪ್ರೂವರ್ ಒಳಗೆ ಚಲಾಯಿಸುತ್ತದೆ. ಪ್ರತಿ ಸ್ಥಿತಿ ಪರಿವರ್ತನೆಯೊಂದಿಗೆ ವ್ಯಾಲಿಡೇಟರ್ ಮತವಲ್ಲ, ಕ್ರಿಪ್ಟೋಗ್ರಾಫಿಕ್ ರಸೀದಿ ಬರುತ್ತದೆ. ಬ್ರಿಡ್ಜ್‌ಗಳಿಲ್ಲದೆ, ನಂಬಿಕೆಯ ಊಹೆಗಳಿಲ್ಲದೆ ಬಿಟ್‌ಕಾಯಿನ್‌ನಲ್ಲಿ ನಿರ್ಮಿಸಿ. ಪ್ರತಿ ಪದರವೂ ಸಂಪೂರ್ಣ ಮುಕ್ತ ಮೂಲ.","hero.start":"ನಿರ್ಮಾಣ ಪ್ರಾರಂಭಿಸಿ","hero.source":"ಮೂಲ ನೋಡಿ",
      "stats.programs":"ಪ್ರೋಗ್ರಾಂಗಳು","stats.sdks":"SDKಗಳು","stats.bridges":"ಬ್ರಿಡ್ಜ್‌ಗಳು","stats.proven":"ಸಾಬೀತು",
      "why.title":"ಗಣಿತ, ಬಹುಮತ ಮತವಲ್ಲ","how.title":"HIMSHA ಹೇಗೆ ಕೆಲಸ ಮಾಡುತ್ತದೆ","programs.title":"ಅಂತರ್ನಿರ್ಮಿತ ಪ್ರೋಗ್ರಾಂಗಳು","usecases.title":"ನೀವು ಏನು ನಿರ್ಮಿಸಬಹುದು","sdks.title":"ನಿಮ್ಮ ಭಾಷೆಯಲ್ಲಿ ನಿರ್ಮಿಸಿ","start.title":"3 ಹಂತಗಳಲ್ಲಿ ಸಿದ್ಧ","docs.title":"ನಿಮಗೆ ಬೇಕಾದ ಎಲ್ಲವೂ",
      "faq.label":"ಪ್ರಶ್ನೆಗಳು","faq.title":"ಪದೇ ಪದೇ ಕೇಳಲಾಗುವ ಪ್ರಶ್ನೆಗಳು","ask.title":"ಪ್ರಶ್ನೆ ಇದೆಯೇ?","ask.qLabel":"ಪ್ರಶ್ನೆ","ask.dLabel":"ವಿವರಗಳು","ask.discuss":"Discussions ನಲ್ಲಿ ಕೇಳಿ","ask.issue":"issue ತೆರೆಯಿರಿ",
      "cta.title":"ಬಿಟ್‌ಕಾಯಿನ್‌ನಲ್ಲಿ ಸಾಬೀತುಪಡಿಸಲು ಪ್ರಾರಂಭಿಸಿ","cta.start":"ಪ್ರಾರಂಭಿಸಿ","cta.tutorials":"ಟ್ಯುಟೋರಿಯಲ್‌ಗಳನ್ನು ಓದಿ",
      "meta.title":"HIMSHA Network — ZK ನಿಂದ ಸಾಬೀತಾದ ಬಿಟ್‌ಕಾಯಿನ್ ಪ್ರೋಗ್ರಾಮೆಬಿಲಿಟಿ","meta.desc":"HIMSHA Network ಸಂಪೂರ್ಣ ಮುಕ್ತ-ಮೂಲ, ZK-ಸಾಬೀತಾದ ಬಿಟ್‌ಕಾಯಿನ್ ಪ್ರೋಗ್ರಾಮೆಬಿಲಿಟಿ ಪದರವಾಗಿದೆ. ಪ್ರತಿ ಸ್ಥಿತಿ ಪರಿವರ್ತನೆಯು ಸರಿಯಾಗಿದೆ ಎಂಬುದನ್ನು ವ್ಯಾಲಿಡೇಟರ್ ಬಹುಮತದಿಂದಲ್ಲ, ಕ್ರಿಪ್ಟೋಗ್ರಾಫಿಕ್ ರಸೀದಿಯಿಂದ ಸಾಬೀತುಪಡಿಸಲಾಗುತ್ತದೆ."
    }
  };

  const LOCALES = [
    { code: 'en',      name: 'English' },
    { code: 'es',      name: 'Español' },
    { code: 'fr',      name: 'Français' },
    { code: 'de',      name: 'Deutsch' },
    { code: 'zh-Hans', name: '简体中文' },
    { code: 'ja',      name: '日本語' },
    { code: 'hi',      name: 'हिन्दी' },
    { code: 'te',      name: 'తెలుగు' },
    { code: 'ta',      name: 'தமிழ்' },
    { code: 'kn',      name: 'ಕನ್ನಡ' },
  ];
  const CODES = LOCALES.map(l => l.code);
  const DEFAULT = 'en';
  const STORE_KEY = 'himsha-lang';

  function resolveInitial() {
    const url = new URLSearchParams(location.search).get('lang');
    if (url && CODES.includes(url)) return url;
    try {
      const saved = localStorage.getItem(STORE_KEY);
      if (saved && CODES.includes(saved)) return saved;
    } catch (e) {}
    const nav = navigator.language || 'en';
    if (CODES.includes(nav)) return nav;
    const base = nav.split('-')[0];
    const match = CODES.find(c => c === base || c.split('-')[0] === base);
    return match || DEFAULT;
  }

  // Snapshot the English baseline from the DOM so switching back restores it.
  let baseline = null;
  function snapshot() {
    if (baseline) return;
    baseline = {};
    document.querySelectorAll('[data-i18n]').forEach(el => { baseline[el.dataset.i18n] = el.textContent; });
    baseline['__title__'] = document.title;
    const md = document.querySelector('meta[name="description"]');
    baseline['__desc__'] = md ? md.getAttribute('content') : '';
  }

  function apply(code) {
    const dict = code === DEFAULT ? null : DICTS[code];
    const get = k => (dict && dict[k] != null) ? dict[k] : (baseline[k] != null ? baseline[k] : null);

    document.querySelectorAll('[data-i18n]').forEach(el => {
      const v = get(el.dataset.i18n);
      if (v != null) el.textContent = v;
    });
    const title = (dict && dict['meta.title']) || baseline['__title__'];
    if (title) document.title = title;
    const md = document.querySelector('meta[name="description"]');
    const desc = (dict && dict['meta.desc']) || baseline['__desc__'];
    if (md && desc) md.setAttribute('content', desc);

    document.documentElement.lang = code;
  }

  function setLang(code, { persist = true, pushUrl = true } = {}) {
    if (!CODES.includes(code)) code = DEFAULT;
    snapshot();
    apply(code);
    if (persist) { try { localStorage.setItem(STORE_KEY, code); } catch (e) {} }
    if (pushUrl) {
      const u = new URL(location.href);
      if (code === DEFAULT) u.searchParams.delete('lang');
      else u.searchParams.set('lang', code);
      history.replaceState(null, '', u);
    }
    document.querySelectorAll('.lang-select').forEach(s => { s.value = code; });
  }

  function mountSwitchers(current) {
    document.querySelectorAll('.lang-mount').forEach(mount => {
      if (mount.querySelector('.lang-select')) return;
      const sel = document.createElement('select');
      sel.className = 'lang-select';
      sel.setAttribute('aria-label', 'Select language');
      LOCALES.forEach(l => {
        const o = document.createElement('option');
        o.value = l.code; o.textContent = l.name;
        sel.appendChild(o);
      });
      sel.value = current;
      sel.addEventListener('change', () => setLang(sel.value));
      mount.appendChild(sel);
    });
  }

  function init() {
    const initial = resolveInitial();
    snapshot();
    mountSwitchers(initial);
    if (initial !== DEFAULT) setLang(initial, { pushUrl: false });
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
