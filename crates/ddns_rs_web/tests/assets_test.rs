use ddns_rs_web::assets::{StaticAssets, TemplateAssets};

#[test]
fn test_static_assets_embedded() {
    let files: Vec<String> = StaticAssets::iter().map(|f| f.into_owned()).collect();
    assert!(!files.is_empty(), "no static assets embedded");
    assert!(
        files.contains(&"common.css".to_string()),
        "common.css not found, got: {:?}",
        files
    );
}

#[test]
fn test_template_assets_embedded() {
    let files: Vec<String> = TemplateAssets::iter().map(|f| f.into_owned()).collect();
    assert!(!files.is_empty(), "no templates embedded");
    assert!(
        files.contains(&"writing.html".to_string()),
        "writing.html not found, got: {:?}",
        files
    );
}

#[test]
fn test_gotemplate_render() {
    use serde_json::json;
    let out = ddns_rs_web::gotemplate::render_template("Hello {{.Name}}!", &json!({"Name": "world"}));
    assert_eq!(out, "Hello world!");
}
