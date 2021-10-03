use tree_sitter::*;

fn is_snake_case(variable_name: &str) -> bool {
    String::from(variable_name).find("_").is_some()
}

fn snake_case_to_camel_case(variable_name: &str) -> String {
    let mut result = String::new();
    let mut uc_flip = false;
    for letter in variable_name.chars() {
        if letter == '_' {
            uc_flip = true;
            continue;
        }
        if uc_flip {
            result.push(letter.to_uppercase().next().unwrap());
        } else {
            result.push(letter);
        }
        uc_flip = false;
    }
    return result;
}

fn edit_source_code(edit: InputEdit, source_code: &str, variable_name: String) -> String {
    let mut new_source_code = String::new();

    new_source_code.push_str(&source_code[..edit.start_byte]);
    new_source_code.push_str(&variable_name);
    new_source_code.push_str(&source_code[edit.old_end_byte..]);

    new_source_code
}

fn shift_input_edit(edit: InputEdit, shift: i32) -> InputEdit {
    let start_byte = edit.start_byte as i32 + shift;
    let old_end_byte = edit.old_end_byte as i32 + shift;
    let new_end_byte = edit.new_end_byte as i32 + shift;

    let start_position = edit.start_position;
    let start_position_column = start_position.column as i32 + shift;

    let old_end_position = edit.old_end_position;
    let old_end_position_column = old_end_position.column as i32 + shift;

    let new_end_position = edit.new_end_position;
    let new_end_position_column = new_end_position.column as i32 + shift;

    InputEdit {
        start_byte: start_byte as usize,
        old_end_byte: old_end_byte as usize,
        start_position: Point { row: start_position.row, column: start_position_column as usize },
        old_end_position: Point { row: old_end_position.row, column: old_end_position_column as usize },
        new_end_position: Point { row: new_end_position.row, column: new_end_position_column as usize },
        new_end_byte: new_end_byte as usize,
    }
}

fn edits_variable_references(tree: &Tree, node: Node, source_code: &str) -> (Vec<InputEdit>, String) {
    let variable_name = node.utf8_text(source_code.as_ref()).unwrap();
    let new_variable_name = snake_case_to_camel_case(variable_name);
    let length_diff: i32 = (new_variable_name.len() as i32) - (variable_name.len() as i32);

    let query_string = format!("((variable_name (name) @var (#match? @var \"{}\")))", variable_name);
    let query = Query::new(tree_sitter_php::language(), &query_string).unwrap();
    let mut query_cursor = QueryCursor::new();
    let matches = query_cursor.matches(&query, tree.root_node(), |n| n.utf8_text(source_code.as_ref()).unwrap());
    let mut edits: Vec<InputEdit> = Vec::new();

    let mut edit_shift: i32 = 0;
    for matched in matches {
        for capture in (matched as QueryMatch).captures {
            let new_end_byte = ((capture.node.end_byte() as i32) + length_diff) as usize;
            let new_end_position = Point {
                row: capture.node.end_position().row,
                column: ((capture.node.end_position().column as i32) + length_diff) as usize,
            };

            let edit = InputEdit {
                start_byte: capture.node.start_byte(),
                old_end_byte: capture.node.end_byte(),
                start_position: capture.node.start_position(),
                old_end_position: capture.node.end_position(),
                new_end_position,
                new_end_byte,
            };
            edits.push(shift_input_edit(edit, edit_shift));
            edit_shift += length_diff;
        }
    }
    return (edits, new_variable_name);
}

fn first_snake_case_variable<'a>(tree: &'a Tree, source_code: &str) -> Option<Node<'a>> {
    // Query it for variables.
    let query = Query::new(tree_sitter_php::language(), "(variable_name (name) @var)").unwrap();
    let mut query_cursor = QueryCursor::new();
    let matches = query_cursor.captures(&query, tree.root_node(), |_| []);

    for matched in matches {
        for capture in matched.0.captures {
            let node: Node = capture.node;
            let variable_name = node.utf8_text(source_code.as_ref()).unwrap();
            if !is_snake_case(variable_name) {
                continue;
            }
            return Some(node);
        }
    }

    return None;
}

fn trash_in_trash_out(mut source_code: String) -> String {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_php::language()).unwrap();
    let mut tree = parser.parse(source_code.as_str(), None).unwrap();

    loop {
        let variable = first_snake_case_variable(&tree, source_code.as_ref());
        if variable.is_none() {
            break;
        }

        let (edits, new_variable_name) = edits_variable_references(&tree, variable.unwrap(), source_code.as_ref());
        for edit in edits {
            tree.edit(&edit);
            source_code = String::from(edit_source_code(edit,
                source_code.as_str(),
                new_variable_name.clone(),
            ));
            tree = parser.parse(source_code.as_str(), Some(&tree)).unwrap();
        }
        // one variable replace happened.
    }
    source_code
}

fn main() {
    // Parse file.
    //let mut source_code = String::from("<?php /*$$<?php$$*/ $a_aa = 222;");
    let source_code = String::from("<?php
    $a_aa = 222;
    function ($a_aa, $r_a) {
        echo $r_a;
    }");

    println!("Inp\n{}", source_code);
    println!("Res\n{}", trash_in_trash_out(source_code));
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    fn assert(param: &str, result: &str) {
        assert_eq!(trash_in_trash_out(param.to_string()), result.to_string());
    }

    #[test]
    fn test_simple() {
        self::assert(
            r#"<?php
                $user_id = 1;
                $user_ = 1;
                $user__ = 1;
            "#,
            r#"<?php
                $userId = 1;
                $user = 1;
                $user = 1;
            "#,
        );
    }

    #[test]
    fn test_plaintext() {
        self::assert("$user_id;", "$user_id;");
    }

    #[test]
    fn test_plaintext_interpolation() {
        self::assert("$user_id; <?= $user_id ?>", "$user_id; <?= $userId ?>");
    }

    #[test]
    fn test_many_vars() {
        self::assert(
            r#"<?php
                $magic_value = array_flip([]);
                $spam_data = [
                    $magic_key => $magic_value,
                ];
            "#,
            r#"<?php
                $magicValue = array_flip([]);
                $spamData = [
                    $magicKey => $magicValue,
                ];
            "#,
        );
    }

    #[test]
    fn test_string_interpolation() {
        self::assert(
            r#"<?php
                "Use the $new_case now.";
                "Use the {$new_case} now.";
                'Try using the $new_case now.';
                'Try using the {$new_case} now.';
                'Try using the ${new_case} now.';
            "#,
            r#"<?php
                "Use the $newCase now.";
                "Use the {$newCase} now.";
                'Try using the $new_case now.';
                'Try using the {$new_case} now.';
                'Try using the ${new_case} now.';
            "#,
        );
    }

    #[allow(dead_code)]
    fn test_string_interpolation_todo() {
        self::assert(
            r#"<?php
                "Use the ${new_case} now.";
            "#,
            r#"<?php
                "Use the ${newCase} now.";
            "#,
        );
    }

    #[test]
    fn test_variable_variable() {
        self::assert(
            r#"<?php
                $do_this = "print joy";
                $never_ever = "do_this";
                echo $$never_ever;
            "#,
            r#"<?php
                $doThis = "print joy";
                $neverEver = "do_this";
                echo $$neverEver;
            "#,
        );
    }

    #[test]
    fn test_dynamic_read() {
        self::assert(
            r#"<?php
                $class->{$some_field};
            "#,
            r#"<?php
                $class->{$someField};
            "#,
        );
    }

    #[test]
    fn test_variable_reading() {
        self::assert(
            r#"<?php
                $do_this[42];
                $do_this->value_that;
            "#,
            r#"<?php
                $doThis[42];
                $doThis->value_that;
            "#,
        );
    }

    #[test]
    fn test_variable_locations() {
        self::assert(
            r#"<?php
                // $user_password
                /** @var Class $user_password */
                $user = funtion snake_case ($user_id, $user_password) use (&$user_location) {
                    return [$user_id, $user_password, $user_location];
                };
            "#,
            r#"<?php
                // $user_password
                /** @var Class $user_password */
                $user = funtion snake_case ($userId, $userPassword) use (&$userLocation) {
                    return [$userId, $userPassword, $userLocation];
                };
            "#,
        );
    }

    #[allow(dead_code)]
    fn test_variable_locations_todo() {
        self::assert(
            r#"<?php
                // $user_password
                /** @var Class $user_password */
                $user = funtion snake_case ($user_id, $user_password) use (&$user_location) {
                    return [$user_id, $user_password, $user_location];
                };
            "#,
            r#"<?php
                // $userPassword
                /** @var Class $userPassword */
                $user = funtion snake_case ($userId, $userPassword) use (&$userLocation) {
                    return [$userId, $userPassword, $userLocation];
                };
            "#,
        );
    }

    #[test]
    fn test_scope_collision() {
        self::assert(
            r#"<?php
            class A {
                funtion b($user_id) {
                    $user_id = 1; // snake case here would be rewritten
                    $userId = 2;
                }
                funtion c() {
                    $userId = 2;
                    $user_id = 1; // snake case here would rewrite above variable
                }
            }
            "#,
            r#"<?php
            class A {
                funtion b($user_id) {
                    $user_id = 1; // snake case here would be rewritten
                    $userId = 2;
                }
                funtion c() {
                    $userId = 2;
                    $user_id = 1; // snake case here would rewrite above variable
                }
            }
            "#,
        );
    }
}
