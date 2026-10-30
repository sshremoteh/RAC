use p256::ecdsa::{signature::Signer, SigningKey, Signature};
use rcgen::{CertificateParams, KeyPair, DistinguishedName, DnType};
use sha2::{Digest, Sha256};
use thiserror::Error;
use rac_zip::find_zip_sections;

#[derive(Error, Debug)]
pub enum SignerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] rac_zip::ZipError),
    #[error("Crypto error: {0}")]
    Crypto(String),
}

const APK_SIGNING_BLOCK_MAGIC: &[u8; 16] = b"APK Sig Block 42";
const APK_SIGNATURE_SCHEME_V2_BLOCK_ID: u32 = 0x7109871a;
const SIGNATURE_ALGO_ECDSA_P256_SHA256: u32 = 0x0201;
const CHUNK_SIZE: usize = 1048576; // 1 MB

/// Generates a test ECDSA P-256 keypair and self-signed X.509 certificate in DER format.
pub fn generate_self_signed_cert() -> Result<(SigningKey, Vec<u8>), SignerError> {
    let mut rng = rand_core::OsRng;
    let signing_key = SigningKey::random(&mut rng);

    // Export private key to PKCS8 PEM to load into rcgen
    let pkcs8_pem = p256::pkcs8::EncodePrivateKey::to_pkcs8_pem(&signing_key, p256::pkcs8::LineEnding::LF)
        .map_err(|e| SignerError::Crypto(e.to_string()))?;

    let key_pair = KeyPair::from_pem(&pkcs8_pem)
        .map_err(|e| SignerError::Crypto(e.to_string()))?;

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "RAC Android Compiler");
    dn.push(DnType::OrganizationName, "Rust Android Compiler");

    let mut params = CertificateParams::default();
    params.distinguished_name = dn;

    let cert = params.self_signed(&key_pair)
        .map_err(|e| SignerError::Crypto(e.to_string()))?;
    let cert_der = cert.der().to_vec();

    Ok((signing_key, cert_der))
}

/// Compute the 1MB chunked SHA-256 digest according to the Android v2 signature specification.
pub fn compute_apk_digest_sha256(
    sec1: &[u8], // bytes before signing block (0..cd_offset)
    sec2: &[u8], // central directory
    sec3_modified_eocd: &[u8], // EOCD with cd_offset pointing to sec1.len()
) -> [u8; 32] {
    let mut chunk_digests = Vec::new();
    let sections = [sec1, sec2, sec3_modified_eocd];

    for sec in sections {
        for chunk in sec.chunks(CHUNK_SIZE) {
            let mut hasher = Sha256::new();
            hasher.update(&[0xa5]);
            hasher.update(&(chunk.len() as u32).to_le_bytes());
            hasher.update(chunk);
            chunk_digests.extend_from_slice(&hasher.finalize());
        }
    }

    let chunk_count = (chunk_digests.len() / 32) as u32;
    let mut final_hasher = Sha256::new();
    final_hasher.update(&[0x5a]);
    final_hasher.update(&chunk_count.to_le_bytes());
    final_hasher.update(&chunk_digests);
    final_hasher.finalize().into()
}

/// Helper to write length-prefixed (4 bytes, little-endian) byte slices.
fn write_len_prefixed(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(data);
}

/// Signs an aligned APK buffer using APK Signature Scheme v2.
pub fn sign_apk_v2(apk_bytes: &[u8]) -> Result<Vec<u8>, SignerError> {
    let (signing_key, cert_der) = generate_self_signed_cert()?;
    sign_apk_v2_with_key(apk_bytes, &signing_key, &cert_der)
}

pub fn sign_apk_v2_with_key(
    apk_bytes: &[u8],
    signing_key: &SigningKey,
    cert_der: &[u8],
) -> Result<Vec<u8>, SignerError> {
    let sections = find_zip_sections(apk_bytes)?;

    let sec1 = &apk_bytes[..sections.cd_offset as usize];
    let sec2 = &apk_bytes[sections.cd_offset as usize..sections.eocd_offset as usize];
    let orig_eocd = &apk_bytes[sections.eocd_offset as usize..];

    // In EOCD, prepare a temporary copy where Central Directory offset points to the original cd_offset
    let mut modified_eocd = orig_eocd.to_vec();
    modified_eocd[16..20].copy_from_slice(&(sections.cd_offset as u32).to_le_bytes());

    // 1. Compute content digest
    let apk_digest = compute_apk_digest_sha256(sec1, sec2, &modified_eocd);

    // 2. Build `signed data`:
    // digests:
    let mut digests_buf = Vec::new();
    {
        let mut single_digest = Vec::new();
        single_digest.extend_from_slice(&SIGNATURE_ALGO_ECDSA_P256_SHA256.to_le_bytes());
        write_len_prefixed(&mut single_digest, &apk_digest);
        write_len_prefixed(&mut digests_buf, &single_digest);
    }

    // certificates:
    let mut certs_buf = Vec::new();
    write_len_prefixed(&mut certs_buf, cert_der);

    // additional attributes (empty):
    let additional_attrs_buf = Vec::new();

    let mut signed_data = Vec::new();
    write_len_prefixed(&mut signed_data, &digests_buf);
    write_len_prefixed(&mut signed_data, &certs_buf);
    write_len_prefixed(&mut signed_data, &additional_attrs_buf);

    // 3. Sign the `signed data` with ECDSA P-256 (ASN.1 DER format for Android v2)
    let verifying_key = *signing_key.verifying_key();
    let signature: Signature = signing_key.sign(&signed_data);
    let der_sig = signature.to_der();

    let mut signatures_buf = Vec::new();
    {
        let mut single_sig = Vec::new();
        single_sig.extend_from_slice(&SIGNATURE_ALGO_ECDSA_P256_SHA256.to_le_bytes());
        write_len_prefixed(&mut single_sig, der_sig.as_bytes());
        write_len_prefixed(&mut signatures_buf, &single_sig);
    }

    // 4. Public key encoding (SubjectPublicKeyInfo DER - Android v2 specification)
    let spki_der = p256::pkcs8::EncodePublicKey::to_public_key_der(&verifying_key)
        .map_err(|e| SignerError::Crypto(e.to_string()))?;
    let pub_key_bytes = spki_der.as_ref();

    // 5. Build `signer`:
    let mut signer_buf = Vec::new();
    write_len_prefixed(&mut signer_buf, &signed_data);
    write_len_prefixed(&mut signer_buf, &signatures_buf);
    write_len_prefixed(&mut signer_buf, pub_key_bytes);

    // 6. Build `signers` sequence:
    let mut signers_buf = Vec::new();
    write_len_prefixed(&mut signers_buf, &signer_buf);

    // 7. Build APK Signature Scheme v2 value (which is a length-prefixed sequence of signers)
    let mut v2_payload = Vec::new();
    write_len_prefixed(&mut v2_payload, &signers_buf);

    // 8. Build APK Signing Block:
    // Pair = [len(uint64) = 4 + len(v2_payload)] [ID(uint32)] [value]
    let pair_value_len = v2_payload.len() as u64;
    let pair_total_len = 4 + pair_value_len;

    // Total size of APK Signing Block (excluding 8 bytes size field at start and at end):
    // ID-value pairs (8 + pair_total_len) + 16 (magic)
    let block_size_excluding_first_size = 8 + pair_total_len + 8 + 16;

    let mut signing_block = Vec::new();
    signing_block.extend_from_slice(&block_size_excluding_first_size.to_le_bytes());
    signing_block.extend_from_slice(&pair_total_len.to_le_bytes());
    signing_block.extend_from_slice(&APK_SIGNATURE_SCHEME_V2_BLOCK_ID.to_le_bytes());
    signing_block.extend_from_slice(&v2_payload);
    signing_block.extend_from_slice(&block_size_excluding_first_size.to_le_bytes());
    signing_block.extend_from_slice(APK_SIGNING_BLOCK_MAGIC);

    // 9. Assemble final signed APK
    let mut final_apk = Vec::new();
    final_apk.extend_from_slice(sec1);
    final_apk.extend_from_slice(&signing_block);
    final_apk.extend_from_slice(sec2);

    // In the final EOCD, update Central Directory offset to point right after the signing block
    let new_cd_offset = (sec1.len() + signing_block.len()) as u32;
    let mut final_eocd = orig_eocd.to_vec();
    final_eocd[16..20].copy_from_slice(&new_cd_offset.to_le_bytes());
    final_apk.extend_from_slice(&final_eocd);

    Ok(final_apk)
}
