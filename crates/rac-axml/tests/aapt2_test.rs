use rac_axml::compile_xml_to_axml;
use std::fs::File;
use std::io::Write;
use std::process::Command;

#[test]
fn test_compile_manifest_against_aapt2() {
    let manifest_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.racdemo"
    android:versionCode="1"
    android:versionName="1.0">
    <uses-sdk android:minSdkVersion="24" android:targetSdkVersion="34" />
    <application android:label="RAC Demo" android:hasCode="false">
        <activity android:name="android.app.NativeActivity"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>"#;

    let axml_bytes = compile_xml_to_axml(manifest_xml).expect("Failed to compile XML to AXML");

    let temp_dir = tempfile::tempdir().unwrap();
    let apk_path = temp_dir.path().join("test.apk");
    {
        let file = File::create(&apk_path).unwrap();
        let mut zip = rac_zip::ZipWriter::new(file);
        zip.add_file("AndroidManifest.xml", &axml_bytes, rac_zip::CompressionMethod::Deflated, None).unwrap();
        zip.finish().unwrap();
    }

    // Verify using official /usr/bin/aapt2 dump xmltree --file AndroidManifest.xml <apk_path>
    let output = Command::new("/usr/bin/aapt2")
        .args(&["dump", "xmltree", "--file", "AndroidManifest.xml", apk_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute aapt2");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("aapt2 stdout:\n{}", stdout);
    println!("aapt2 stderr:\n{}", stderr);

    assert!(output.status.success(), "aapt2 dump failed: {}{}", stdout, stderr);
    assert!(stdout.contains("manifest"));
    assert!(stdout.contains("com.example.racdemo"));
    assert!(stdout.contains("NativeActivity"));
}
