use rac_zip::{ZipWriter, CompressionMethod};
use rac_signer::sign_apk_v2;
use std::fs::File;
use std::io::Write;
use std::process::Command;

#[test]
fn test_against_system_apksigner() {
    let temp_dir = tempfile::tempdir().unwrap();
    let raw_apk_path = temp_dir.path().join("raw.apk");
    let signed_apk_path = temp_dir.path().join("signed.apk");

    let manifest_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.racdemo"
    android:versionCode="1"
    android:versionName="1.0">
    <uses-sdk android:minSdkVersion="24" android:targetSdkVersion="34" />
    <application android:label="RAC Demo" android:hasCode="false">
    </application>
</manifest>"#;
    let axml_bytes = rac_axml::compile_xml_to_axml(manifest_xml).unwrap();

    // 1. Build a basic aligned APK
    {
        let file = File::create(&raw_apk_path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.add_file("AndroidManifest.xml", &axml_bytes, CompressionMethod::Deflated, None).unwrap();
        zip.add_file("classes.dex", b"dex\n035\0dummy_bytecode_payload", CompressionMethod::Deflated, None).unwrap();
        zip.add_file("resources.arsc", b"dummy_arsc_resource_table", CompressionMethod::Stored, Some(4)).unwrap();
        zip.finish().unwrap();
    }

    let raw_bytes = std::fs::read(&raw_apk_path).unwrap();

    // 2. Sign it using RAC signer
    let signed_bytes = sign_apk_v2(&raw_bytes).expect("Signing failed");
    {
        let mut file = File::create(&signed_apk_path).unwrap();
        file.write_all(&signed_bytes).unwrap();
    }

    // 3. Verify using official Google apksigner (autodetects min-sdk from compiled AndroidManifest.xml!)
    let output = Command::new("/usr/bin/apksigner")
        .args(&["verify", "--verbose", signed_apk_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute /usr/bin/apksigner");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("apksigner stdout:\n{}", stdout);
    println!("apksigner stderr:\n{}", stderr);

    assert!(output.status.success(), "apksigner verification failed! Output: {}{}", stdout, stderr);
    assert!(stdout.contains("Verifies"));
    assert!(stdout.contains("Verified using v2 scheme (APK Signature Scheme v2): true"));
}
