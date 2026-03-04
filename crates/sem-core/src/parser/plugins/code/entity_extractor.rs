use tree_sitter::{Node, Tree};

use crate::model::entity::{build_entity_id, SemanticEntity};
use crate::utils::hash::{content_hash, structural_hash};
use super::languages::LanguageConfig;

pub fn extract_entities(
    tree: &Tree,
    file_path: &str,
    config: &LanguageConfig,
    source_code: &str,
) -> Vec<SemanticEntity> {
    let mut entities = Vec::new();
    visit_node(
        tree.root_node(),
        file_path,
        config,
        &mut entities,
        None,
        source_code.as_bytes(),
    );
    entities
}

fn visit_node(
    node: Node,
    file_path: &str,
    config: &LanguageConfig,
    entities: &mut Vec<SemanticEntity>,
    parent_id: Option<&str>,
    source: &[u8],
) {
    let node_type = node.kind();

    if config.entity_node_types.contains(&node_type) {
        if let Some(name) = extract_name(node, source) {
            let entity_type = if node_type == "decorated_definition" {
                map_decorated_type(node)
            } else {
                map_node_type(node_type)
            };
            let content_str = node_text(node, source);
            let content = content_str.to_string();

            let struct_hash = structural_hash(node, source);
            let entity = SemanticEntity {
                id: build_entity_id(file_path, entity_type, &name, parent_id),
                file_path: file_path.to_string(),
                entity_type: entity_type.to_string(),
                name: name.clone(),
                parent_id: parent_id.map(String::from),
                content_hash: content_hash(&content),
                structural_hash: Some(struct_hash),
                content,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                metadata: None,
            };

            let entity_id = entity.id.clone();
            entities.push(entity);

            // Visit children for nested entities (methods inside classes, etc.)
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if config.container_node_types.contains(&child.kind()) {
                    let mut inner_cursor = child.walk();
                    for nested in child.named_children(&mut inner_cursor) {
                        visit_node(
                            nested,
                            file_path,
                            config,
                            entities,
                            Some(&entity_id),
                            source,
                        );
                    }
                }
            }
            return;
        }
    }

    // For export statements, look inside for the actual declaration
    if node_type == "export_statement" {
        if let Some(declaration) = node.child_by_field_name("declaration") {
            visit_node(declaration, file_path, config, entities, parent_id, source);
            return;
        }
    }

    // Recurse into top-level children
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_node(child, file_path, config, entities, parent_id, source);
    }
}

fn extract_name(node: Node, source: &[u8]) -> Option<String> {
    // Try 'name' field first (works for most languages)
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(node_text(name_node, source).to_string());
    }

    // For variable/lexical declarations, try to get the declarator name
    let node_type = node.kind();
    if node_type == "lexical_declaration" || node_type == "variable_declaration" {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let Some(decl_name) = child.child_by_field_name("name") {
                    return Some(node_text(decl_name, source).to_string());
                }
            }
        }
    }

    // For decorated definitions (Python), look at the inner definition
    if node_type == "decorated_definition" {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "function_definition" || child.kind() == "class_definition" {
                if let Some(inner_name) = child.child_by_field_name("name") {
                    return Some(node_text(inner_name, source).to_string());
                }
            }
        }
    }

    // For C/C++ function_definition, the name is inside the declarator
    if node_type == "function_definition" {
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return extract_declarator_name(declarator, source);
        }
    }

    // For C++ template_declaration, look at the inner declaration
    if node_type == "template_declaration" {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            let kind = child.kind();
            if kind != "template_parameter_list" {
                // The inner declaration (class, function, etc.)
                if let Some(name) = child.child_by_field_name("name") {
                    return Some(node_text(name, source).to_string());
                }
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    return extract_declarator_name(declarator, source);
                }
            }
        }
    }

    // For C++ namespace_definition
    if node_type == "namespace_definition" {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(node_text(name_node, source).to_string());
        }
    }

    // For C++ class_specifier
    if node_type == "class_specifier" {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(node_text(name_node, source).to_string());
        }
    }

    // For C# property_declaration, namespace_declaration, struct_declaration
    if node_type == "property_declaration" || node_type == "namespace_declaration" || node_type == "struct_declaration" {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(node_text(name_node, source).to_string());
        }
    }

    // For C declarations (global vars, function prototypes), extract the declarator name
    if node_type == "declaration" {
        if let Some(declarator) = node.child_by_field_name("declarator") {
            // Could be a plain identifier, pointer_declarator, function_declarator, etc.
            return extract_declarator_name(declarator, source);
        }
    }

    // For C struct/enum/union specifiers, try the 'name' field
    if node_type == "struct_specifier" || node_type == "enum_specifier" || node_type == "union_specifier" {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(node_text(name_node, source).to_string());
        }
    }

    // For C type_definition (typedef), look for the type name
    if node_type == "type_definition" {
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return extract_declarator_name(declarator, source);
        }
    }

    // Fallback: first identifier child
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "type_identifier" {
            return Some(node_text(child, source).to_string());
        }
    }

    None
}

/// Extract the name from a C declarator (handles pointer_declarator, function_declarator, etc.)
fn extract_declarator_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" | "field_identifier" => Some(node_text(node, source).to_string()),
        "qualified_identifier" | "scoped_identifier" => {
            // For C++ qualified names like ClassName::method, return the full qualified name
            Some(node_text(node, source).to_string())
        }
        "pointer_declarator" | "function_declarator" | "array_declarator" | "parenthesized_declarator" => {
            if let Some(inner) = node.child_by_field_name("declarator") {
                extract_declarator_name(inner, source)
            } else {
                let mut cursor = node.walk();
                let result = node.named_children(&mut cursor)
                    .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
                    .map(|c| node_text(c, source).to_string());
                result
            }
        }
        _ => {
            if let Some(name) = node.child_by_field_name("name") {
                return Some(node_text(name, source).to_string());
            }
            let mut cursor = node.walk();
            let result = node.named_children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
                .map(|c| node_text(c, source).to_string());
            result
        }
    }
}

fn node_text<'a>(node: Node, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn map_node_type<'a>(tree_sitter_type: &'a str) -> &'a str {
    match tree_sitter_type {
        "function_declaration" | "function_definition" | "function_item" => "function",
        "method_declaration" | "method_definition" | "method" | "singleton_method" => "method",
        "class_declaration" | "class_definition" | "class_specifier" => "class",
        "interface_declaration" => "interface",
        "type_alias_declaration" | "type_declaration" | "type_item" | "type_definition" => "type",
        "enum_declaration" | "enum_item" | "enum_specifier" => "enum",
        "struct_item" | "struct_specifier" | "struct_declaration" => "struct",
        "union_specifier" => "union",
        "impl_item" => "impl",
        "trait_item" => "trait",
        "mod_item" | "module" | "namespace_definition" | "namespace_declaration" => "module",
        "export_statement" => "export",
        "lexical_declaration" | "variable_declaration" | "var_declaration" | "declaration" => "variable",
        "const_declaration" | "const_item" => "constant",
        "static_item" => "static",
        "decorated_definition" => "decorated_definition",
        "constructor_declaration" => "constructor",
        "field_declaration" | "public_field_definition" | "field_definition" => "field",
        "property_declaration" => "property",
        "annotation_type_declaration" => "annotation",
        "template_declaration" => "template",
        other => other,
    }
}

/// For Python decorated_definition, check the inner node to determine the real type.
fn map_decorated_type(node: Node) -> &'static str {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "class_definition" => return "class",
            "function_definition" => return "function",
            _ => {}
        }
    }
    "function"
}
