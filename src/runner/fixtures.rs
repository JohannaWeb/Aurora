use std::path::PathBuf;

pub(crate) fn fixture_url(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("fixtures");
    path.push(name);
    path.push("index.html");
    format!("file://{}", path.display())
}

pub(crate) fn demo_html() -> &'static str {
    r#"<!DOCTYPE html>
<html>
<head>
<style>
  body { background-color: #F0F4F8; margin: 0; padding: 32px; }
  h1 { color: #d26428; font-size: 48px; background-color: #FFF3E0; padding: 16px; margin: 0 0 16px 0; }
  h2 { color: #ffffff; font-size: 28px; background-color: #2E3440; padding: 10px 16px; margin: 24px 0 8px 0; }
  p  { font-size: 18px; color: #333333; background-color: #ffffff; padding: 8px 12px; margin: 4px 0; }
  .red   { background-color: #ff6b6b; color: #ffffff; padding: 12px 16px; margin: 4px 0; font-size: 18px; }
  .green { background-color: #40c057; color: #ffffff; padding: 12px 16px; margin: 4px 0; font-size: 18px; }
  .blue  { background-color: #228be6; color: #ffffff; padding: 12px 16px; margin: 4px 0; font-size: 18px; }
</style>
</head>
<body>
  <h1>Aurora Browser</h1>

  <h2>Render Test</h2>
  <p>If you can read this, the rendering pipeline is working correctly.</p>

  <h2>Colored Blocks</h2>
  <div class="red">Red block — background rendering test</div>
  <div class="green">Green block — background rendering test</div>
  <div class="blue">Blue block — background rendering test</div>

  <h2>Typography</h2>
  <p>The quick brown fox jumps over the lazy dog. 0 1 2 3 4 5 6 7 8 9</p>
  <p>Uppercase: A B C D E F G H I J K L M N O P Q R S T U V W X Y Z</p>
  <p>Lowercase: a b c d e f g h i j k l m n o p q r s t u v w x y z</p>

  <h2>Word Wrap</h2>
  <p>This paragraph tests word wrapping. Aurora uses a word-wrap algorithm to break long lines of text into multiple lines that each fit within the available viewport width.</p>

  <h2>Unicode</h2>
  <p>Arrows: left right up down northeast southwest</p>
  <p>Symbols: copyright registered trademark degree</p>
  <p>Math: plus-minus multiply divide approximately not-equal infinity</p>
</body>
</html>"#
}
