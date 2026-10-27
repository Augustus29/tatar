# Prototip dosyaları

Bunlar Claude tasarım kanvasındaki ekranların kaynak dosyalarıdır (.dc.html).
Her biri tek başına bir ekran: HTML + satır içi stiller + küçük bir mantık sınıfı (`renderVals`).
`{{...}}` delikleri, `<sc-if>` ve `<sc-for>` etiketleri kanvasın şablon sözdizimidir; React'e çevirirken
stil değerlerini, metinleri, durumları ve etkileşimleri koru, sözdizimini taşıma.
`MainDark`, `MainSolid` ve `MainWindows`, `Main.dc.html`'in props ile üretilmiş kopyalarıdır.
