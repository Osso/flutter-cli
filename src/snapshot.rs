use anyhow::Result;

use crate::isolate;
use crate::vm_service::VmServiceConnection;

#[derive(Clone)]
pub struct SnapshotOptions {
    pub max_depth: Option<usize>,
    pub filter: Option<String>,
    pub compact: bool,
}

/// A node in the Flutter widget tree (DiagnosticsNode from the inspector protocol).
#[derive(Debug, Clone)]
pub struct WidgetNode {
    pub widget_type: String,
    pub value_id: String,
    pub description: String,
    pub creation_location: Option<CreationLocation>,
    pub children: Vec<WidgetNode>,
}

#[derive(Debug, Clone)]
pub struct CreationLocation {
    pub file: String,
    pub line: u32,
}

pub async fn get_widget_tree(conn: &mut VmServiceConnection) -> Result<Vec<WidgetNode>> {
    let isolate_id = isolate::find_flutter_isolate(conn).await?;
    let object_group = "flutter-cli-snapshot";

    let result = conn
        .send(
            "ext.flutter.inspector.getRootWidgetSummaryTree",
            serde_json::json!({
                "isolateId": isolate_id,
                "objectGroup": object_group,
            }),
        )
        .await?;

    let tree = parse_diagnostics_node(&result);

    // Clean up the object group
    let _ = conn
        .send(
            "ext.flutter.inspector.disposeGroup",
            serde_json::json!({
                "isolateId": isolate_id,
                "objectGroup": object_group,
            }),
        )
        .await;

    Ok(match tree {
        Some(node) => vec![node],
        None => vec![],
    })
}

fn parse_diagnostics_node(value: &serde_json::Value) -> Option<WidgetNode> {
    let description = value
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();

    let widget_type = value
        .get("widgetRuntimeType")
        .or_else(|| value.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let value_id = value
        .get("valueId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let creation_location = value.get("creationLocation").and_then(parse_location);

    let children = value
        .get("children")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().filter_map(parse_diagnostics_node).collect())
        .unwrap_or_default();

    Some(WidgetNode {
        widget_type,
        value_id,
        description,
        creation_location,
        children,
    })
}

fn parse_location(loc: &serde_json::Value) -> Option<CreationLocation> {
    let file = loc.get("file").and_then(|f| f.as_str())?;
    let line = loc.get("line").and_then(|l| l.as_u64())? as u32;

    // Extract just the filename from the full path
    let filename = file.rsplit('/').next().unwrap_or(file);

    Some(CreationLocation {
        file: filename.to_string(),
        line,
    })
}

/// Format the widget tree as indented text.
pub fn format_tree(nodes: &[WidgetNode], opts: &SnapshotOptions) -> String {
    let mut lines = Vec::new();
    for node in nodes {
        if opts.filter.is_some() {
            collect_filtered_subtrees(node, opts, &mut lines);
        } else {
            format_node(node, 0, opts, &mut lines);
        }
    }
    lines.join("\n")
}

/// Known framework-internal widget types to skip in compact mode.
const FRAMEWORK_WIDGETS: &[&str] = &[
    "Semantics",
    "MergeSemantics",
    "DefaultTextStyle",
    "AnimatedDefaultTextStyle",
    "MediaQuery",
    "Localizations",
    "FocusScope",
    "FocusTrap",
    "FocusTraversalGroup",
    "Actions",
    "Shortcuts",
    "PrimaryScrollController",
    "UnmanagedRestorationScope",
    "RestorationScope",
    "ScrollConfiguration",
    "HeroControllerScope",
    "IconTheme",
    "ListTileTheme",
    "_InheritedTheme",
    "Theme",
    "AnimatedTheme",
    "Builder",
    "RepaintBoundary",
    "NotificationListener",
    "KeepAlive",
    "AutomaticKeepAlive",
    "KeyedSubtree",
    "Offstage",
    "TickerMode",
    "ColoredBox",
    "DecoratedBox",
    "ConstrainedBox",
    "UnconstrainedBox",
    "LimitedBox",
    "SizedBox",
    "Expanded",
    "Flexible",
    "Positioned",
    "Align",
    "Center",
    "Padding",
    "SliverPadding",
    "Material",
    "InkWell",
    "Ink",
    "CustomPaint",
    "PhysicalModel",
    "PhysicalShape",
    "ClipRect",
    "ClipRRect",
    "ClipPath",
    "ClipOval",
    "Transform",
    "Opacity",
    "AnimatedOpacity",
    "FadeTransition",
    "SizeTransition",
    "SlideTransition",
    "ScaleTransition",
    "RotationTransition",
    "AnimatedContainer",
    "AnimatedBuilder",
    "StreamBuilder",
    "FutureBuilder",
    "ValueListenableBuilder",
    "LayoutBuilder",
    "OrientationBuilder",
    "SliverToBoxAdapter",
    "SliverList",
    "SliverFixedExtentList",
    "SliverFillRemaining",
    "CustomScrollView",
    "Scrollable",
    "Viewport",
    "ShrinkWrappingViewport",
    "_BodyBuilder",
    "_ScaffoldSlot",
];

fn is_framework_widget(widget_type: &str) -> bool {
    // Check direct match
    if FRAMEWORK_WIDGETS.contains(&widget_type) {
        return true;
    }
    // Widgets starting with _ are private/framework-internal
    if widget_type.starts_with('_') {
        return true;
    }
    false
}

fn format_node(node: &WidgetNode, depth: usize, opts: &SnapshotOptions, lines: &mut Vec<String>) {
    if let Some(max) = opts.max_depth {
        if depth > max {
            return;
        }
    }

    // Compact mode: skip framework internals, promote children
    if opts.compact && is_framework_widget(&node.widget_type) {
        for child in &node.children {
            format_node(child, depth, opts, lines);
        }
        return;
    }

    let indent = "  ".repeat(depth);
    let mut line = format!("{}{}", indent, node.widget_type);

    // Show text content for Text widgets
    if node.widget_type == "Text" && !node.description.is_empty() && node.description != "Text" {
        let text = node
            .description
            .strip_prefix("Text")
            .unwrap_or(&node.description)
            .trim();
        if !text.is_empty() {
            // Remove surrounding quotes if already present
            let text = text.trim_matches('"');
            line.push_str(&format!(" \"{}\"", text));
        }
    }

    // Value ID
    if !node.value_id.is_empty() {
        line.push_str(&format!("  [{}]", node.value_id));
    }

    // Source location
    if let Some(ref loc) = node.creation_location {
        line.push_str(&format!(" {}:{}", loc.file, loc.line));
    }

    lines.push(line);

    for child in &node.children {
        format_node(child, depth + 1, opts, lines);
    }
}

fn name_matches_filter(name: &str, filter: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    let filter_lower = filter.to_ascii_lowercase();
    if filter.contains('*') {
        glob_match(&filter_lower, &name_lower)
    } else {
        name_lower.contains(&filter_lower)
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = text[pos..].find(part) {
            if i == 0 && idx != 0 {
                return false;
            }
            pos += idx + part.len();
        } else {
            return false;
        }
    }
    !parts.last().is_some_and(|p| !p.is_empty()) || pos == text.len()
}

fn collect_filtered_subtrees(node: &WidgetNode, opts: &SnapshotOptions, lines: &mut Vec<String>) {
    let filter = opts.filter.as_deref().unwrap_or("");
    if name_matches_filter(&node.widget_type, filter) {
        let no_filter_opts = SnapshotOptions {
            filter: None,
            ..opts.clone()
        };
        format_node(node, 0, &no_filter_opts, lines);
    } else {
        for child in &node.children {
            collect_filtered_subtrees(child, opts, lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_opts() -> SnapshotOptions {
        SnapshotOptions {
            max_depth: None,
            filter: None,
            compact: false,
        }
    }

    fn make_widget(widget_type: &str, value_id: &str, children: Vec<WidgetNode>) -> WidgetNode {
        WidgetNode {
            widget_type: widget_type.to_string(),
            value_id: value_id.to_string(),
            description: String::new(),
            creation_location: None,
            children,
        }
    }

    fn make_widget_with_loc(
        widget_type: &str,
        value_id: &str,
        file: &str,
        line: u32,
        children: Vec<WidgetNode>,
    ) -> WidgetNode {
        WidgetNode {
            widget_type: widget_type.to_string(),
            value_id: value_id.to_string(),
            description: String::new(),
            creation_location: Some(CreationLocation {
                file: file.to_string(),
                line,
            }),
            children,
        }
    }

    fn make_text(text: &str, value_id: &str) -> WidgetNode {
        WidgetNode {
            widget_type: "Text".to_string(),
            value_id: value_id.to_string(),
            description: format!("Text \"{}\"", text),
            creation_location: None,
            children: vec![],
        }
    }

    #[test]
    fn basic_tree() {
        let tree = vec![make_widget(
            "MaterialApp",
            "inspector-0",
            vec![make_widget(
                "Scaffold",
                "inspector-2",
                vec![make_text("Hello", "inspector-4")],
            )],
        )];
        let output = format_tree(&tree, &default_opts());
        assert_eq!(
            output,
            "MaterialApp  [inspector-0]\n\
             \x20\x20Scaffold  [inspector-2]\n\
             \x20\x20\x20\x20Text \"Hello\"  [inspector-4]"
        );
    }

    #[test]
    fn with_source_location() {
        let tree = vec![make_widget_with_loc(
            "MyWidget",
            "inspector-0",
            "my_widget.dart",
            42,
            vec![],
        )];
        let output = format_tree(&tree, &default_opts());
        assert_eq!(output, "MyWidget  [inspector-0] my_widget.dart:42");
    }

    #[test]
    fn max_depth() {
        let tree = vec![make_widget(
            "L0",
            "i0",
            vec![make_widget(
                "L1",
                "i1",
                vec![make_widget(
                    "L2",
                    "i2",
                    vec![make_widget("L3", "i3", vec![])],
                )],
            )],
        )];
        let opts = SnapshotOptions {
            max_depth: Some(2),
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        assert_eq!(output, "L0  [i0]\n  L1  [i1]\n    L2  [i2]");
    }

    #[test]
    fn compact_skips_framework_widgets() {
        let tree = vec![make_widget(
            "MyApp",
            "i0",
            vec![make_widget(
                "Padding",
                "i1",
                vec![make_widget(
                    "Center",
                    "i2",
                    vec![make_widget("MyButton", "i3", vec![])],
                )],
            )],
        )];
        let opts = SnapshotOptions {
            compact: true,
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        // Padding and Center are framework widgets, skipped in compact mode
        assert_eq!(output, "MyApp  [i0]\n  MyButton  [i3]");
    }

    #[test]
    fn compact_skips_private_widgets() {
        let tree = vec![make_widget(
            "Scaffold",
            "i0",
            vec![make_widget(
                "_ScaffoldLayout",
                "i1",
                vec![make_widget("AppBar", "i2", vec![])],
            )],
        )];
        let opts = SnapshotOptions {
            compact: true,
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        assert_eq!(output, "Scaffold  [i0]\n  AppBar  [i2]");
    }

    #[test]
    fn filter_substring() {
        let tree = vec![make_widget(
            "App",
            "i0",
            vec![
                make_widget("ComicCard", "i1", vec![make_text("Batman", "i2")]),
                make_widget("NavBar", "i3", vec![]),
                make_widget(
                    "ComicList",
                    "i4",
                    vec![make_widget(
                        "ComicCard",
                        "i5",
                        vec![make_text("Superman", "i6")],
                    )],
                ),
            ],
        )];
        let opts = SnapshotOptions {
            filter: Some("ComicCard".to_string()),
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        assert_eq!(
            output,
            "ComicCard  [i1]\n\
             \x20\x20Text \"Batman\"  [i2]\n\
             ComicCard  [i5]\n\
             \x20\x20Text \"Superman\"  [i6]"
        );
    }

    #[test]
    fn filter_case_insensitive() {
        let tree = vec![make_widget("NavBar", "i0", vec![])];
        let opts = SnapshotOptions {
            filter: Some("navbar".to_string()),
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        assert_eq!(output, "NavBar  [i0]");
    }

    #[test]
    fn filter_glob_prefix() {
        let tree = vec![make_widget(
            "App",
            "i0",
            vec![
                make_widget("ComicCard", "i1", vec![]),
                make_widget("ComicList", "i2", vec![]),
                make_widget("NavBar", "i3", vec![]),
            ],
        )];
        let opts = SnapshotOptions {
            filter: Some("Comic*".to_string()),
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        assert_eq!(output, "ComicCard  [i1]\nComicList  [i2]");
    }

    #[test]
    fn filter_glob_suffix() {
        let tree = vec![make_widget(
            "App",
            "i0",
            vec![
                make_widget("ComicCard", "i1", vec![]),
                make_widget("ArtistCard", "i2", vec![]),
                make_widget("NavBar", "i3", vec![]),
            ],
        )];
        let opts = SnapshotOptions {
            filter: Some("*Card".to_string()),
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        assert_eq!(output, "ComicCard  [i1]\nArtistCard  [i2]");
    }

    #[test]
    fn filter_no_match() {
        let tree = vec![make_widget(
            "App",
            "i0",
            vec![make_widget("NavBar", "i1", vec![])],
        )];
        let opts = SnapshotOptions {
            filter: Some("DoesNotExist".to_string()),
            ..default_opts()
        };
        let output = format_tree(&tree, &opts);
        assert!(output.is_empty());
    }

    #[test]
    fn glob_match_exact() {
        assert!(glob_match("hello", "hello"));
        assert!(!glob_match("hello", "world"));
    }

    #[test]
    fn glob_match_wildcard() {
        assert!(glob_match("comic*", "comiccard"));
        assert!(glob_match("*card", "comiccard"));
        assert!(glob_match("*mic*", "comiccard"));
        assert!(glob_match("comic*card", "comiccard"));
        assert!(!glob_match("comic*", "navbarcomic"));
        assert!(!glob_match("*card", "cardnav"));
    }

    #[test]
    fn empty_tree() {
        let output = format_tree(&[], &default_opts());
        assert!(output.is_empty());
    }
}
