use rac_zip::{ZipWriter, CompressionMethod};
use std::fs::File;
use std::process::Command;

#[test]
fn test_against_system_zipalign() {
    let temp_dir = tempfile::tempdir().unwrap();
    let apk_path = temp_dir.path().join("test_aligned.apk");

    {
        let file = File::create(&apk_path).unwrap();
        let mut zip = ZipWriter::new(file);

        // Android APK rules:
        // 1. Uncompressed files (stored) should be 4-byte aligned
        // 2. Compressed files (deflated) don't need alignment
        zip.add_file("AndroidManifest.xml", b"<fake_manifest_bytes>", CompressionMethod::Deflated, None).unwrap();
        zip.add_file("resources.arsc", b"<resources_table_must_be_aligned>", CompressionMethod::Stored, Some(4)).unwrap();
        zip.add_file("lib/arm64-v8a/libmain.so", b"<elf_native_binary_must_be_aligned>", CompressionMethod::Stored, Some(4)).unwrap();
        zip.add_file("assets/data.bin", b"<random_payload_data>", CompressionMethod::Deflated, None).unwrap();

        zip.finish().unwrap();
    }

    // Run official /usr/bin/zipalign -c -v 4 <apk>
    let output = Command::new("/usr/bin/zipalign")
        .args(&["-c", "-v", "4", apk_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute /usr/bin/zipalign");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("zipalign stdout:\n{}", stdout);
    println!("zipalign stderr:\n{}", stderr);

    assert!(output.status.success(), "zipalign verification failed! Output: {}{}", stdout, stderr);
}
