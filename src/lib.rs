// rwtodo is it possible to return &str with lifetime specifiers?
// rwtodo how does this handle 日本語 etc?
pub fn common_prefix(names: &[String]) -> String {
    let name_a = &names[0];

    let mut prefix_len = name_a.len();
    for name_b in names {
        while prefix_len > 0 && !name_b.starts_with(&name_a[..prefix_len]) {
            prefix_len -= 1;
        }
    }

    name_a[..prefix_len].to_string()
}

pub fn common_suffix(names: &[String]) -> String {
    let name_a = &names[0];

    let mut suffix_start = 0;
    for name_b in names {
        while suffix_start < name_a.len() && !name_b.ends_with(&name_a[suffix_start..]) {
            suffix_start += 1;
        }
    }

    name_a[suffix_start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_ends() {
        let test_data_list = [
            (["abc efg", "abcdefg"], "abc", "efg"),
            (["ag", "abcdefg"], "a", "g"),
            (["cde", "abcdefg"], "", ""),
        ];

        for test_data in test_data_list {
            let result = common_prefix(&test_data.0);
            assert_eq!(result, test_data.1);
            let result = common_suffix(&test_data.0);
            assert_eq!(result, test_data.2);
        }
    }
}
