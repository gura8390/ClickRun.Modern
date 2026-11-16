// Adapted from the existing local OCR project's resource writer; no SDK rc.exe needed.
use std::{env, fs, io, path::PathBuf};
const ICON: &[u8] = include_bytes!("assets/clickrun.ico");
const MANIFEST: &[u8] = include_bytes!("assets/app.manifest");
fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=assets/clickrun.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let path =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR")).join("application.res");
    fs::write(&path, resources())?;
    println!("cargo:rustc-link-arg-bin=liandianqi={}", path.display());
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Keep the full UIA provider available, loading its DLL transparently on
        // the first Uia* call rather than when the application image is loaded.
        // Apply both flags only to the final application, not test binaries.
        println!("cargo:rustc-link-arg-bin=liandianqi=/DELAYLOAD:uiautomationcore.dll");
        println!("cargo:rustc-link-arg-bin=liandianqi=/DEFAULTLIB:delayimp.lib");
    }
    Ok(())
}
fn align4(output: &mut Vec<u8>) {
    output.resize(output.len().next_multiple_of(4), 0);
}
fn append_resource(output: &mut Vec<u8>, kind: u16, name: u16, language: u16, data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_le_bytes());
    output.extend_from_slice(&32u32.to_le_bytes());
    for id in [kind, name] {
        output.extend_from_slice(&u16::MAX.to_le_bytes());
        output.extend_from_slice(&id.to_le_bytes());
    }
    output.extend_from_slice(&0u32.to_le_bytes());
    output.extend_from_slice(&0x0030u16.to_le_bytes());
    output.extend_from_slice(&language.to_le_bytes());
    output.extend_from_slice(&[0; 8]);
    output.extend_from_slice(data);
    align4(output);
}
fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}
fn version_node(
    key: &str,
    kind: u16,
    value_length: u16,
    value: &[u8],
    children: &[Vec<u8>],
) -> Vec<u8> {
    let mut node = vec![0; 2];
    node.extend_from_slice(&value_length.to_le_bytes());
    node.extend_from_slice(&kind.to_le_bytes());
    node.extend_from_slice(&wide(key));
    align4(&mut node);
    node.extend_from_slice(value);
    for child in children {
        align4(&mut node);
        node.extend_from_slice(child);
    }
    let len: u16 = node.len().try_into().expect("version resource too large");
    node[..2].copy_from_slice(&len.to_le_bytes());
    node
}
fn version_resource() -> Vec<u8> {
    let values = [
        ("FileDescription", "连点器 Modern"),
        ("ProductName", "连点器 Modern"),
        ("FileVersion", "3.0.1"),
        ("ProductVersion", "3.0.1"),
        ("InternalName", "liandianqi"),
        ("OriginalFilename", "连点器 Modern.exe"),
    ];
    let strings: Vec<Vec<u8>> = values
        .iter()
        .map(|(name, value)| {
            let bytes = wide(value);
            version_node(name, 1, (bytes.len() / 2) as u16, &bytes, &[])
        })
        .collect();
    let table = version_node("080404B0", 1, 0, &[], &strings);
    let string_info = version_node("StringFileInfo", 1, 0, &[], &[table]);
    let var = version_node("Translation", 0, 4, &[0x04, 0x08, 0xb0, 0x04], &[]);
    let var_info = version_node("VarFileInfo", 1, 0, &[], &[var]);
    let fixed = [
        0xFEEF04BDu32,
        0x00010000,
        0x00030000,
        0x00010000,
        0x00030000,
        0x00010000,
        0x3f,
        0,
        0x00040004,
        1,
        0,
        0,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect::<Vec<_>>();
    version_node(
        "VS_VERSION_INFO",
        0,
        fixed.len() as u16,
        &fixed,
        &[string_info, var_info],
    )
}
fn resources() -> Vec<u8> {
    assert_eq!(&ICON[..4], &[0, 0, 1, 0], "invalid ICO");
    let count = u16::from_le_bytes([ICON[4], ICON[5]]);
    assert!(count > 0, "empty ICO");
    let mut result = Vec::new();
    append_resource(&mut result, 0, 0, 0, &[]);
    let mut group = ICON[..6].to_vec();
    for id in 1..=count {
        let start = 6 + 16 * usize::from(id - 1);
        let entry = &ICON[start..start + 16];
        let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        append_resource(&mut result, 3, id, 0, &ICON[offset..offset + length]);
        group.extend_from_slice(&entry[..12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    append_resource(&mut result, 14, 1, 0, &group);
    append_resource(&mut result, 24, 1, 0, MANIFEST);
    append_resource(&mut result, 16, 1, 0x0804, &version_resource());
    result
}
