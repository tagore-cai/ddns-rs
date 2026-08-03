use serde_json::Value;

/// A minimal Go html/template renderer supporting the constructs used by
/// ddns-go's templates: {{.Field}}, {{range .Field}}..{{end}},
/// {{if .Field}}..{{else}}..{{end}}, {{if len .Field}}, and trim markers.

enum Node {
    Text(String),
    Output(String),
    If {
        cond: Cond,
        then_branch: Vec<Node>,
        else_branch: Vec<Node>,
    },
    Range {
        field: String,
        body: Vec<Node>,
    },
}

enum Cond {
    Field(String),
    Len(String),
}

fn parse_nodes(src: &str) -> Vec<Node> {
    fn parse_into<'a>(src: &'a str, until: &'a str) -> (Vec<Node>, Option<String>, &'a str) {
        let mut nodes = Vec::new();
        let mut text_buf = String::new();
        let mut rest = src;
        loop {
            if let Some(pos) = rest.find("{{") {
                text_buf.push_str(&rest[..pos]);
                rest = &rest[pos + 2..];
                if let Some(end) = rest.find("}}") {
                    let cmd = &rest[..end];
                    rest = &rest[end + 2..];
                    let (trim_pre, cmd2) = cmd.strip_prefix('-').map(|c| (true, c)).unwrap_or((false, cmd));
                    let (trim_post, cmd3) = cmd2
                        .strip_suffix('-')
                        .map(|c| (true, c))
                        .unwrap_or((false, cmd2));
                    let cmd3 = cmd3.trim();
                    if trim_pre {
                        while text_buf.ends_with(|c: char| c.is_whitespace()) {
                            text_buf.pop();
                        }
                    }
                    if !text_buf.is_empty() {
                        nodes.push(Node::Text(std::mem::take(&mut text_buf)));
                    }

                    if cmd3 == until {
                        return (nodes, Some(cmd3.to_string()), rest);
                    } else if cmd3 == "end" {
                        return (nodes, Some("end".to_string()), rest);
                    } else if let Some(field) = cmd3.strip_prefix("range .") {
                        let (body, closing, rest2) = parse_into(rest, "end");
                        debug_assert_eq!(closing.as_deref(), Some("end"));
                        nodes.push(Node::Range {
                            field: field.trim().to_string(),
                            body,
                        });
                        rest = rest2;
                    } else if let Some(field) = cmd3.strip_prefix("if len .") {
                        let (then_branch, closing, rest2) = parse_into(rest, "else");
                        let (else_branch, rest3) = if closing.as_deref() == Some("else") {
                            let (else_nodes, _, rest3) = parse_into(rest2, "end");
                            (else_nodes, rest3)
                        } else {
                            (Vec::new(), rest2)
                        };
                        nodes.push(Node::If {
                            cond: Cond::Len(field.trim().to_string()),
                            then_branch,
                            else_branch,
                        });
                        rest = rest3;
                    } else if let Some(field) = cmd3.strip_prefix("if .") {
                        let (then_branch, closing, rest2) = parse_into(rest, "else");
                        let (else_branch, rest3) = if closing.as_deref() == Some("else") {
                            let (else_nodes, _, rest3) = parse_into(rest2, "end");
                            (else_nodes, rest3)
                        } else {
                            (Vec::new(), rest2)
                        };
                        nodes.push(Node::If {
                            cond: Cond::Field(field.trim().to_string()),
                            then_branch,
                            else_branch,
                        });
                        rest = rest3;
                    } else if let Some(field) = cmd3.strip_prefix('.') {
                        nodes.push(Node::Output(field.trim().to_string()));
                    } else {
                        // Unknown tag; keep as text
                        text_buf.push_str(&format!("{{{{{}}}}}", cmd3));
                    }
                    if trim_post {
                        while rest.starts_with(|c: char| c.is_whitespace()) {
                            rest = &rest[1..];
                        }
                    }
                } else {
                    text_buf.push_str(&rest[..]);
                    rest = "";
                    break;
                }
            } else {
                text_buf.push_str(rest);
                rest = "";
                break;
            }
        }
        if !text_buf.is_empty() {
            nodes.push(Node::Text(text_buf));
        }
        (nodes, None, rest)
    }

    let (parsed, _, _) = parse_into(src, "end");
    parsed
}

fn render_nodes(nodes: &[Node], ctx: &Value, out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Output(field) => {
                if let Some(v) = ctx.get(field) {
                    render_value(v, out);
                }
            }
            Node::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let truthy = match cond {
                    Cond::Field(field) => match ctx.get(field) {
                        Some(Value::Bool(b)) => *b,
                        Some(Value::String(s)) => !s.is_empty(),
                        Some(Value::Array(a)) => !a.is_empty(),
                        Some(Value::Null) => false,
                        _ => false,
                    },
                    Cond::Len(field) => match ctx.get(field) {
                        Some(Value::Array(a)) => !a.is_empty(),
                        Some(Value::String(s)) => !s.is_empty(),
                        _ => false,
                    },
                };
                if truthy {
                    render_nodes(then_branch, ctx, out);
                } else {
                    render_nodes(else_branch, ctx, out);
                }
            }
            Node::Range { field, body } => {
                if let Some(Value::Array(items)) = ctx.get(field) {
                    for item in items {
                        render_nodes(body, item, out);
                    }
                }
            }
        }
    }
}

fn render_value(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => out.push_str(s),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        // Go html/template renders slices like "[a b c]"
        Value::Array(a) => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                render_value(item, out);
            }
            out.push(']');
        }
        Value::Null => {}
        Value::Object(_) => {}
    }
}

/// Render a Go template with the given JSON context.
pub fn render_template(src: &str, ctx: &Value) -> String {
    let nodes = parse_nodes(src);
    let mut out = String::new();
    render_nodes(&nodes, ctx, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_field() {
        let out = render_template("Hello {{.Name}}!", &json!({"Name": "world"}));
        assert_eq!(out, "Hello world!");
    }

    #[test]
    fn test_range() {
        let tpl = "<option value=\"{{.Name}}\">{{.Name}}{{.Address}}</option>";
        let ctx = json!({
            "Ipv4": [
                {"Name": "en0", "Address": ["1.2.3.4"]},
                {"Name": "en1", "Address": []}
            ]
        });
        // Wrap in range
        let full = format!("{{{{range .Ipv4}}}}{}{{{{end}}}}", tpl);
        let out = render_template(&full, &ctx);
        assert_eq!(
            out,
            "<option value=\"en0\">en0[1.2.3.4]</option><option value=\"en1\">en1[]</option>"
        );
    }

    #[test]
    fn test_if_else() {
        let tpl = "{{if .NotAllowWanAccess}}checked{{end}}";
        assert_eq!(
            render_template(tpl, &json!({"NotAllowWanAccess": true})),
            "checked"
        );
        assert_eq!(render_template(tpl, &json!({"NotAllowWanAccess": false})), "");

        let tpl2 = "{{if len .Ipv4}}has{{else}}empty{{end}}";
        assert_eq!(
            render_template(tpl2, &json!({"Ipv4": [1, 2]})),
            "has"
        );
        assert_eq!(render_template(tpl2, &json!({"Ipv4": []})), "empty");
    }

    #[test]
    fn test_trim_markers() {
        let tpl = "<button data-i18n=\"{{- if .EmptyUser -}}LoginInit{{else}}Login{{- end -}}\">";
        let out = render_template(tpl, &json!({"EmptyUser": true}));
        assert_eq!(out, "<button data-i18n=\"LoginInit\">");
        let out = render_template(tpl, &json!({"EmptyUser": false}));
        assert_eq!(out, "<button data-i18n=\"Login\">");
    }
}
