# Verify your download

Silo's **Windows** installer and application are digitally signed (Authenticode, via Azure
Trusted/Artifact Signing). The signature proves two things:

- **Who published the files** — a verified identity, not an anonymous upload.
- **That they haven't been altered** since they were signed.

Silo is published by **HLLMR Ventures LLC** (Texas, USA). The Windows code-signing
certificate is held personally by the company's founder, so Windows shows that personal
name as the publisher.

The publisher shown by Windows is:

```
David Hellmer
```

(full certificate subject: `CN=David Hellmer, O=David Hellmer, L=Grand Prairie, S=tx, C=US`)

> **What signing does and doesn't prove.** It proves identity and integrity — that these
> exact bytes came from this publisher. It is **not** a guarantee the software is safe or
> "virus-free," and it is not a Microsoft endorsement. Silo is open source; you can read or
> build every line yourself.
>
> New releases may still trigger a Windows SmartScreen prompt while the publisher's
> reputation builds — but it will show the verified publisher above, **not** "unknown
> publisher," and it eases as clean installs accumulate under the same identity.

macOS and Linux builds are **not** OS-signed yet (Gatekeeper may warn); verify those by
building from source or by the SHA-256 checksums below.

---

## Verify the signature (Windows)

### In Explorer
1. Right-click the downloaded installer (`Silo_x.y.z_x64-setup.exe` or the `.msi`) → **Properties**.
2. Open the **Digital Signatures** tab.
3. Confirm the signer is **David Hellmer**, select it → **Details**, and confirm Windows
   reports **"This digital signature is OK."**

### In PowerShell
```powershell
Get-AuthenticodeSignature ".\Silo_x.y.z_x64-setup.exe" |
    Select-Object Status,
        @{Name = "Publisher"; Expression = { $_.SignerCertificate.Subject }}
```
Expected:
```
Status : Valid
Publisher : CN=David Hellmer, O=David Hellmer, L=Grand Prairie, S=tx, C=US
```

## Verify the checksum (any OS)

Every release attaches `SHA256SUMS.txt`. Download it alongside your installer and compare.

- **Windows (PowerShell):** `Get-FileHash .\Silo_x.y.z_x64-setup.exe -Algorithm SHA256`
- **macOS/Linux:** `shasum -a 256 Silo_x.y.z_amd64.AppImage`

The printed hash should match the line for that file in `SHA256SUMS.txt`.

## Verify build provenance (any OS)

Releases carry a GitHub build attestation tying the artifact to this repository, commit, and
CI workflow. With the [GitHub CLI](https://cli.github.com):

```bash
gh attestation verify <the-file-you-downloaded> --repo HLLMR-VENTURES/silo
```

Silo 1.0.1 and earlier were built while the repository was `HLLMR/silo`, and their attestations
stayed with that account when the repository moved. For those downloads, use:

```bash
gh attestation verify <the-file-you-downloaded> --owner HLLMR --signer-repo HLLMR/silo
```

A successful verification confirms the file was produced by Silo's public build pipeline. It
does not, by itself, prove the software is safe — it proves *where it came from*.

## See what's inside (SBOM)

Each release also attaches `silo-sbom.cdx.json` — a CycloneDX Software Bill of Materials
listing every Rust crate and npm package (with licenses) that goes into the build. Open it in
any SBOM viewer, or scan it for known vulnerabilities with your tool of choice.
