use fxlauncher::java::{extract_major_version, parse_java_version, required_java_major};

#[test]
fn test_parse_java_8() {
    let output = r#"
java version "1.8.0_312"
Java(TM) SE Runtime Environment (build 1.8.0_312-b07)
Java HotSpot(TM) 64-Bit Server VM (build 25.312-b07, mixed mode)
"#;
    let (ver_str, major, is_64) = parse_java_version(output).unwrap();
    assert_eq!(ver_str, "1.8.0_312");
    assert_eq!(major, 8);
    assert!(is_64);
}

#[test]
fn test_parse_java_17() {
    let output = r#"
openjdk version "17.0.9" 2023-10-17
OpenJDK Runtime Environment (build 17.0.9+9)
OpenJDK 64-Bit Server VM (build 17.0.9+9, mixed mode, sharing)
"#;
    let (ver_str, major, is_64) = parse_java_version(output).unwrap();
    assert_eq!(ver_str, "17.0.9");
    assert_eq!(major, 17);
    assert!(is_64);
}

#[test]
fn test_parse_java_21() {
    let output = r#"
openjdk version "21.0.2" 2024-01-16
OpenJDK Runtime Environment (build 21.0.2+13-58)
OpenJDK 64-Bit Server VM (build 21.0.2+13-58, mixed mode, sharing)
"#;
    let (ver_str, major, is_64) = parse_java_version(output).unwrap();
    assert_eq!(ver_str, "21.0.2");
    assert_eq!(major, 21);
    assert!(is_64);
}

#[test]
fn test_parse_java_32_bit() {
    let output = r#"
java version "1.8.0_201"
Java(TM) SE Runtime Environment (build 1.8.0_201-b09)
Java HotSpot(TM) 32-Bit Client VM (build 25.201-b09, mixed mode)
"#;
    let (ver_str, major, is_64) = parse_java_version(output).unwrap();
    assert_eq!(ver_str, "1.8.0_201");
    assert_eq!(major, 8);
    assert!(!is_64);
}

#[test]
fn test_extract_major_version() {
    assert_eq!(extract_major_version("1.8.0_312"), Some(8));
    assert_eq!(extract_major_version("1.7.0_80"), Some(7));
    assert_eq!(extract_major_version("11.0.12"), Some(11));
    assert_eq!(extract_major_version("17.0.9"), Some(17));
    assert_eq!(extract_major_version("21.0.2"), Some(21));
    assert_eq!(extract_major_version("25-ea"), Some(25));
}

#[test]
fn test_required_java_major() {
    assert_eq!(required_java_major("1.12.2"), 8);
    assert_eq!(required_java_major("1.16.5"), 8);
    assert_eq!(required_java_major("1.17"), 16);
    assert_eq!(required_java_major("1.17.1"), 16);
    assert_eq!(required_java_major("1.18.2"), 17);
    assert_eq!(required_java_major("1.19.4"), 17);
    assert_eq!(required_java_major("1.20.1"), 17);
    assert_eq!(required_java_major("1.20.6"), 21);
    assert_eq!(required_java_major("1.21"), 21);
    assert_eq!(required_java_major("1.21.4"), 21);
}
