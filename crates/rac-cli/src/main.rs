use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use rac_axml::compile_xml_to_axml;
use rac_dex::generate_empty_dex;
use rac_signer::{generate_self_signed_cert, generate_v1_signature_files, sign_apk_v2, sign_apk_v2_with_key};
use rac_zip::{find_zip_sections, CompressionMethod, ZipWriter};
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const MINIMAL_ARM64_SO: &[u8] = include_bytes!("res/libmain.so");

#[derive(Parser)]
#[command(name = "rac", author = "RAC Team", version, about = "Rust Android Compiler - Ultra-fast standalone APK compiler and packager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new minimal Android project
    New {
        /// Project directory path
        path: PathBuf,
        /// Application package name (e.g. com.example.myapp)
        #[arg(long, default_value = "com.example.racapp")]
        package: String,
        /// Application display name
        #[arg(long, default_value = "RAC App")]
        name: String,
    },
    /// Build and sign an APK from a project directory
    Build {
        /// Path to project directory
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Output APK path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Sign an existing aligned APK using APK Signature Scheme v2
    Sign {
        /// Input APK path
        input: PathBuf,
        /// Output signed APK path (defaults to overwriting input or <input>-signed.apk)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Verify APK ZIP integrity and structure
    Verify {
        /// Path to the APK
        apk: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { path, package, name } => handle_new(&path, &package, &name),
        Commands::Build { project, output } => handle_build(&project, output.as_deref()),
        Commands::Sign { input, output } => handle_sign(&input, output.as_deref()),
        Commands::Verify { apk } => handle_verify(&apk),
    }
}

fn handle_new(path: &Path, package: &str, name: &str) -> Result<()> {
    if path.exists() {
        bail!("Directory '{}' already exists", path.display());
    }

    fs::create_dir_all(path.join("lib/arm64-v8a"))?;
    fs::create_dir_all(path.join("assets"))?;
    fs::create_dir_all(path.join("res"))?;

    let manifest = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{package}"
    android:versionCode="1"
    android:versionName="1.0">

    <uses-sdk android:minSdkVersion="24" android:targetSdkVersion="34" />

    <application
        android:label="{name}"
        android:hasCode="false">
        <activity
            android:name="android.app.NativeActivity"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#
    );

    fs::write(path.join("AndroidManifest.xml"), manifest)?;
    // Write real, valid 64-bit ARM ELF shared library
    fs::write(path.join("lib/arm64-v8a/libmain.so"), MINIMAL_ARM64_SO)?;

    println!("Created new Android project in '{}'", path.display());
    println!("  Package: {package}");
    println!("  App Name: {name}");
    println!("  Native Library: lib/arm64-v8a/libmain.so (Valid ARM64 ELF)");
    println!("Run 'rac build {}' to compile this project into an APK.", path.display());
    Ok(())
}

struct ProjectFile {
    path_in_apk: String,
    data: Vec<u8>,
    method: CompressionMethod,
    alignment: Option<u32>,
}

fn handle_build(project: &Path, output: Option<&Path>) -> Result<()> {
    let manifest_path = project.join("AndroidManifest.xml");
    if !manifest_path.exists() {
        bail!("No AndroidManifest.xml found in '{}'", project.display());
    }

    println!("Compiling manifest...");
    let manifest_xml = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let axml_bytes = compile_xml_to_axml(&manifest_xml)
        .context("Failed to compile AndroidManifest.xml to binary XML")?;

    let out_apk = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let app_name = project
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "app".to_string());
            project.join(format!("{app_name}-release.apk"))
        }
    };

    let mut project_files: Vec<ProjectFile> = Vec::new();

    // 1. AndroidManifest.xml
    project_files.push(ProjectFile {
        path_in_apk: "AndroidManifest.xml".to_string(),
        data: axml_bytes,
        method: CompressionMethod::Deflated,
        alignment: None,
    });

    // 2. classes.dex
    let dex_path = project.join("classes.dex");
    let dex_bytes = if dex_path.exists() {
        println!("  Adding classes.dex from project");
        fs::read(&dex_path)?
    } else {
        println!("  Generating default minimal DEX stub");
        generate_empty_dex().context("Failed to generate minimal DEX")?
    };
    project_files.push(ProjectFile {
        path_in_apk: "classes.dex".to_string(),
        data: dex_bytes,
        method: CompressionMethod::Deflated,
        alignment: None,
    });

    // 3. Native libraries (lib/**)
    let lib_dir = project.join("lib");
    if lib_dir.exists() {
        for entry in WalkDir::new(&lib_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(project)?;
                let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                let data = fs::read(entry.path())?;
                println!("  Adding aligned native lib: {rel_str}");
                project_files.push(ProjectFile {
                    path_in_apk: rel_str,
                    data,
                    method: CompressionMethod::Stored,
                    alignment: Some(4),
                });
            }
        }
    }

    // 4. Assets (assets/**)
    let assets_dir = project.join("assets");
    if assets_dir.exists() {
        for entry in WalkDir::new(&assets_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(project)?;
                let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                let data = fs::read(entry.path())?;
                println!("  Adding asset: {rel_str}");
                project_files.push(ProjectFile {
                    path_in_apk: rel_str,
                    data,
                    method: CompressionMethod::Deflated,
                    alignment: None,
                });
            }
        }
    }

    // 5. Resources (res/**)
    let res_dir = project.join("res");
    if res_dir.exists() {
        for entry in WalkDir::new(&res_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(project)?;
                let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                let data = fs::read(entry.path())?;
                println!("  Adding resource: {rel_str}");
                project_files.push(ProjectFile {
                    path_in_apk: rel_str,
                    data,
                    method: CompressionMethod::Deflated,
                    alignment: None,
                });
            }
        }
    }

    println!("Generating certificate & v1 JAR signatures (META-INF)...");
    let (signing_key, cert_der) = generate_self_signed_cert()
        .context("Failed to generate signing certificate")?;

    // Prepare files list for v1 signature calculation
    let file_refs: Vec<(&str, &[u8])> = project_files
        .iter()
        .map(|f| (f.path_in_apk.as_str(), f.data.as_slice()))
        .collect();

    let v1_meta_files = generate_v1_signature_files(&file_refs, &signing_key, &cert_der)
        .context("Failed to generate v1 signature files")?;

    println!("Packaging APK with 4-byte alignment...");
    let mut zip_buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut zip_buf);

        // First add user files
        for f in &project_files {
            zip.add_file(&f.path_in_apk, &f.data, f.method, f.alignment)?;
        }

        // Add META-INF v1 signature files
        for (name, data) in &v1_meta_files {
            zip.add_file(name, data, CompressionMethod::Deflated, None)?;
        }

        zip.finish().context("Failed to finalize ZIP archive")?;
    }

    let unsigned_bytes = zip_buf.into_inner();

    println!("Signing APK with APK Signature Scheme v2 (ECDSA-P256)...");
    let signed_bytes = sign_apk_v2_with_key(&unsigned_bytes, &signing_key, &cert_der)
        .context("Failed to sign APK with v2 scheme")?;

    let mut out_file = File::create(&out_apk)
        .with_context(|| format!("Failed to create output file '{}'", out_apk.display()))?;
    out_file.write_all(&signed_bytes)?;

    println!("Success! APK built and signed with both v1 (JAR) and v2 schemes:");
    println!("  -> {}", out_apk.display());
    println!("  Size: {} bytes", signed_bytes.len());
    Ok(())
}

fn handle_sign(input: &Path, output: Option<&Path>) -> Result<()> {
    let apk_bytes = fs::read(input).with_context(|| format!("Failed to read {}", input.display()))?;

    println!("Signing '{}' with APK Signature Scheme v2...", input.display());
    let signed_bytes = sign_apk_v2(&apk_bytes).context("Signing failed")?;

    let target_path = match output {
        Some(p) => p.to_path_buf(),
        None => input.to_path_buf(),
    };

    fs::write(&target_path, &signed_bytes)?;
    println!("Saved signed APK to '{}'", target_path.display());
    Ok(())
}

fn handle_verify(apk: &Path) -> Result<()> {
    let data = fs::read(apk).with_context(|| format!("Failed to read {}", apk.display()))?;
    let sections = find_zip_sections(&data).context("Invalid ZIP/APK structure")?;

    println!("APK verification for '{}':", apk.display());
    println!("  Total size: {} bytes", data.len());
    println!("  Central Directory offset: {}", sections.cd_offset);
    println!("  Central Directory size: {} bytes", sections.cd_size);
    println!("  EOCD offset: {}", sections.eocd_offset);

    // Check for APK Signing Block
    if sections.cd_offset >= 32 {
        let magic_pos = (sections.cd_offset - 16) as usize;
        let magic = &data[magic_pos..sections.cd_offset as usize];
        if magic == b"APK Sig Block 42" {
            println!("  APK Signing Block (v2): PRESENT (Magic OK)");
        } else {
            println!("  APK Signing Block: Not present (unsigned or v1-only)");
        }
    }

    Ok(())
}
