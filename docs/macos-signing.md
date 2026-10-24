# Signing and notarizing the Mac app

Without this, a downloaded `Neko-<ver>-macos-universal.dmg` is ad-hoc signed and macOS says
*Apple could not verify "Neko" is free of malware* on first launch. Signed with a Developer ID
and notarized, it opens with no prompt. The release workflow does both on its own once the five
secrets below exist; until then it keeps building ad-hoc signed releases.

`packaging/install-mac.sh` avoids the prompt today: files fetched with `curl` carry no quarantine
flag, so Gatekeeper never asks. It is the warning-free option on the website's Download section.

## 1. Join the Apple Developer Program

Enroll at <https://developer.apple.com/programs/enroll/> as an individual ($99 a year). Approval
usually takes a day or two. Only the Account Holder can create Developer ID certificates.

## 2. Create a Developer ID Application certificate

On your Mac, in Xcode: **Settings › Accounts**, select your team, **Manage Certificates…**, then
**+ › Developer ID Application**. (Without Xcode: make a certificate signing request in Keychain
Access, upload it at <https://developer.apple.com/account/resources/certificates/add>, choose
*Developer ID Application*, and open the downloaded `.cer`.)

## 3. Export it as a .p12

In **Keychain Access › login › My Certificates**, right-click
*Developer ID Application: Your Name (TEAMID)* › **Export…**, save as `DeveloperID.p12` and set a
password. Keep the file private and delete it once the secret is set.

## 4. Collect the rest

- **Team ID**: <https://developer.apple.com/account> › Membership details (10 characters).
- **App-specific password**: <https://account.apple.com> › Sign-In and Security ›
  App-Specific Passwords › **+**, name it `neko-notary`.

## 5. Add the GitHub secrets

```bash
R=bharath914/neko-proxy
base64 -i DeveloperID.p12 | gh secret set MACOS_CERT_P12 -R $R
gh secret set MACOS_CERT_PASSWORD -R $R      # the .p12 password
gh secret set APPLE_ID -R $R                 # your Apple Account email
gh secret set APPLE_TEAM_ID -R $R            # e.g. AB12CD34EF
gh secret set APPLE_APP_PASSWORD -R $R       # the app-specific password
rm DeveloperID.p12
```

## 6. Check it, then release

Run the release workflow by hand on `main` (Actions › release › Run workflow). The `macos` job
should log `status: Accepted`, staple the ticket, and pass `spctl --assess`. Then bump the version
in `Cargo.toml`, merge, and push the matching tag (`git tag v0.1.2 && git push origin v0.1.2`).
Publish the draft and the new `.dmg` opens without a prompt.

## Local builds

```bash
xcrun notarytool store-credentials neko-notary --apple-id you@example.com --team-id AB12CD34EF
SIGN_IDENTITY="Developer ID Application: Your Name (AB12CD34EF)" NOTARY_PROFILE=neko-notary \
  UNIVERSAL=1 packaging/macos/build.sh
```
