# Wallet passes

A party to a yup can keep it in their phone's wallet: Apple Wallet on an iPhone, Google Wallet on Android (`DESIGN.md` §11). The pass is their view of that one exchange, kept up to date as it changes, with a link back to it. It is optional, it never lets anyone in by itself, and it holds nothing from the agreement.

Everything is built and tested with throwaway credentials. Nothing has been issued to a real device: that needs the Apple and Google accounts, and then the steps under "Once the accounts exist". Until a deployment sets a platform's settings, that platform is off and nobody sees anything of it.

The code is `backend/src/wallet/` (with the routes in `backend/src/http/wallet.rs`), `packages/shared/src/wallet.ts`, and the button in each app (`apps/web/src/components/WalletButton.tsx`, `apps/mobile/src/components/WalletButton.tsx`).

## What a pass shows

A wallet shows a pass without the phone being unlocked, and a pass that has reached a device cannot be taken back, so the face is held to the lock-screen rule of `DESIGN.md` §11 and §12: generic words only, nothing from the terms.

| On the pass | What it is |
|---|---|
| Product | Yuppers, with the mark |
| Status (the large field) | How the agreement stands for the person holding the pass: *Waiting for you* (the other party marked something delivered to them, proposed a change, proposed ending, or asked to close), *Disputed*, *Overdue*, *Due soon* (within the reminders' lead, two days), *In force*; once closed, *Completed*, *Ended by agreement* or *Closed*, with the day it closed |
| Reference | The exchange's display code |
| Next due | The earliest date of a contribution still open, while it is in force |
| Still to do | How many contributions are still open |
| With | The other party's alias, only when one is set. Their name, as the agreement writes it, is never used in its place, so today, with no way to set an alias, this line does not appear |
| Back | A link to the exchange, which asks its reader to sign in, and one sentence saying the terms stay in Yuppers |

Never on it: what anyone gives or pays, an amount, a description, the terms, a note, a name. The labels and words come from the wording files, `wallet` in `packages/shared/wording/{language}.json`, in the account's language (English and Spanish today); a date is spelled from the same file, so both wallets show the same. The face is one pure function, `wallet::pass::render`, from the exchange as its party sees it, and both platforms draw that one model (`backend/src/wallet/pass/tests.rs` checks it in both languages, and that nothing from the agreement gets onto it).

**Silent updates.** A change to a pass never raises an alert of its own: Apple's fields carry no `changeMessage`, and nothing asks Google to notify. The person is told about the event by its notification, once (§12, no duplicates). §11 makes status on the lock screen something a person opts into; nothing offers that choice yet, so only the default is built, and a `changeMessage` on the status field is where the option would go.

**Revocation.** When an account is deleted its passes are voided as part of the deletion (`backend/src/deletion.rs`): each is updated once to a void face that says only that it is no longer in use, without the reference, the link or anything else, then never again. Apple's pass is marked `voided`, and its devices are forgotten once told; Google's object goes `INACTIVE`. A device that was offline keeps what it last showed, which is why the face is minimal to begin with.

## How it works

**Endpoints**, for a party to the exchange once something has been agreed (anyone else is told the exchange does not exist; before agreement, `ACTION_NOT_ALLOWED`; a platform that is not configured, `WALLET_UNAVAILABLE`, 404):

| | |
|---|---|
| `POST /v1/exchanges/{id}/wallet/apple` | The signed `.pkpass` itself (`application/vnd.apple.pkpass`) |
| `POST /v1/exchanges/{id}/wallet/apple/link` | A link that downloads that pass for ten minutes without a session: `{web origin}/v1/wallet/apple/pass?token=…`. The token is an HMAC under `APP_SECRET` of the pass and the expiry, and travels in the query string, which the service never logs. This is what the apps use: Safari opening the link is what adds the pass to Wallet |
| `POST /v1/exchanges/{id}/wallet/google` | The "Save to Google Wallet" link, `https://pay.google.com/gp/v/save/<JWT>` |
| `GET /v1/meta` | `wallet_platforms`: the platforms configured, `APPLE`, `GOOGLE`, or none |

Each party gets one pass per exchange per platform, the same one every time they ask, and may ask ten times an hour per pass (`WalletRules`), which bounds how often a script can make the service sign.

**Apple's pass web service**, under the `webServiceURL` a pass names, `{web origin}/v1/wallet/apple`: a device registers for a pass's updates and unregisters, asks which of its passes changed since a tag, fetches the latest pass with `If-Modified-Since` (304 when unchanged), and sends its logs. A device proves it holds a pass with the pass's authentication token (`Authorization: ApplePass <token>`), which is derived from the pass and `APP_SECRET`, so handing the pass out again gives the same token and copies on other devices keep working; only its SHA-256 is stored. These routes are Apple's protocol, not part of the API description the clients are generated from.

**The pass.** `generic` style: a store card is for balances and points, and an event ticket, a boarding pass or a coupon each imply something this is not; generic shows the status large and the reference and counts beneath it, and matches Google's generic pass. Its files are `pass.json`, the icon and logo at three scales (drawn from the Yuppers mark by `apps/mobile/scripts/make-icons.mjs` into `backend/assets/wallet/`), `manifest.json` with each file's SHA-1, and `signature`: a detached CMS `SignedData` over the manifest, SHA-256 with RSA, carrying the pass type certificate and the WWDR intermediate, with the content type, signing time and digest as signed attributes. The archive's entries are stored, not compressed.

**Google.** A generic class, `{issuer}.yuppers_agreement`, and one generic object per pass, `{issuer}.{serial}`, both carried in the save link's JWT (RS256, signed with the service account's key, `aud` `google`, `typ` `savetowallet`, `origins` the web origin); Google creates them when the person saves the pass. The class limits a pass to one person's devices. Whether to ask Google for its private pass type instead is open (§11); the object would change in little but its type name.

**Storage.** `wallet_pass` (from migration 0001) is one row per pass; migration `0010_wallet.sql` adds the authentication token's hash, the face's time and hashes, the update queue's columns and the hourly count, and `wallet_device_registration` for Apple's devices. The serial number is random and says nothing about the exchange or the person.

**Updates.** Every change to an exchange marks its passes `PENDING` in the transaction that records it (`exchanges::repo::persist` calls `wallet::store::mark_exchange_changed`), the way the outbox queues a message with its event. The pass row is its own queue entry: many changes in a row make one update, and an update always carries the latest face, so nothing that reaches a device can go backwards. The worker (`wallet::delivery`, on every 5-second pass) takes marked passes on a two-minute lease rather than a lock held while it sends, so marking a pass never waits on a push. It draws the face as it now is; if that is what was last delivered, nothing is sent (a statement added, say, changes nothing on the pass). Otherwise, for Apple, the pass's Last-Modified moves on and each registered device gets an empty push through APNs, after which the device fetches the pass; a token Apple says is dead is forgotten. For Google, the object is patched through the Google Wallet API with an access token for the service account; an object Google does not have, because the person never saved the pass, needs nothing. A failure is retried with backoff, 8 times, and then the pass is left `FAILED` with the error in `last_error` until the exchange changes again.

**What it is built on.** RSA signatures are ring's (constant-time, and already the TLS provider here); the CMS structures are the `cms` crate's from RustCrypto, used without its `builder` feature so that the `rsa` crate, whose timing advisory has no fix (RUSTSEC-2023-0071), is not in the service. HTTP to Apple and Google is hyper with rustls (`hyper-rustls`), HTTP/2 to APNs with the pass type certificate as the client certificate. The tests make their RSA keys with the `rsa` crate, as a development dependency only, which the advisory itself says is fine for local use.

## Settings

| Setting | |
|---|---|
| `APPLE_PASS_TYPE_ID` | The pass type identifier, such as `pass.app.yuppers` |
| `APPLE_TEAM_ID` | The Apple team ID, ten letters and digits |
| `APPLE_PASS_CERT` | The pass type certificate, PEM: the text itself, or the path of a file holding it |
| `APPLE_PASS_KEY` | Its private key, unencrypted PEM (PKCS #8 or PKCS #1), text or path. A secret |
| `APPLE_WWDR_CERT` | Apple's WWDR intermediate certificate that issued the pass type certificate, PEM, text or path |
| `GOOGLE_WALLET_ISSUER_ID` | The issuer ID, digits |
| `GOOGLE_WALLET_SERVICE_ACCOUNT` | The service account's JSON key file: its path, or the JSON itself. A secret |
| `WALLET_DELIVERY` | How the worker sends updates once a platform is on: `live` (APNs and the Google Wallet API) or `log` (written to the worker's log, for development). Required then, with no default, like the other deliveries |

A platform is on when all of its settings are set and off when none are; anything in between, a key that is not the certificate's, a certificate issued for another pass type or team, or an expired certificate stops the process at start. A certificate within 30 days of expiry is logged as a warning at start. Both the api and the worker read the same settings: the api signs passes and answers devices, the worker pushes and patches. The web origin (`WEB_ORIGIN`) is where passes link to and where Apple's devices reach the web service, so it must be the public HTTPS origin the API is served from.

## Once the accounts exist

In order. None of it can be done before the accounts exist (`DESIGN.md` §11).

### Apple

1. **Pass type identifier.** In the Apple Developer account, Certificates, Identifiers & Profiles, Identifiers, add a *Pass Type ID*: `pass.app.yuppers` (the description is for you).
2. **Certificate.** On a machine you trust, make a key and a signing request:

   ```sh
   openssl req -new -newkey rsa:2048 -nodes -keyout pass.key -out pass.csr -subj "/CN=Yuppers Pass Type"
   ```

   In Certificates, add a *Pass Type ID Certificate* for that identifier, upload `pass.csr`, download `pass.cer`, and convert it: `openssl x509 -inform der -in pass.cer -out pass.pem`. (Keychain Access can make the request instead; then export the certificate with its key as a `.p12` and take the two apart with `openssl pkcs12 -in pass.p12 -clcerts -nokeys -out pass.pem` and `openssl pkcs12 -in pass.p12 -nocerts -nodes -out pass.key`.) `pass.key` is the secret: put it in the platform's secret store and delete the local copy.
3. **WWDR intermediate.** From Apple's certificate authority page (apple.com/certificateauthority), download the *Worldwide Developer Relations* intermediate that issued `pass.pem` (`openssl x509 -in pass.pem -noout -issuer` names it; G4 at the time of writing) and convert it: `openssl x509 -inform der -in AppleWWDRCAG4.cer -out wwdr.pem`.
4. **Calendar.** Pass type certificates expire (a year, roughly; `openssl x509 -in pass.pem -noout -enddate`). An expired one stops every update and every new pass. Put the date on the shared calendar with its owner and backup, and renew a month ahead: a new certificate for the same identifier keeps every issued pass working.
5. **Settings.** `APPLE_PASS_TYPE_ID=pass.app.yuppers`, `APPLE_TEAM_ID`, `APPLE_PASS_CERT`, `APPLE_PASS_KEY`, `APPLE_WWDR_CERT`, and `WALLET_DELIVERY=live`, on the api and the worker; restart both.

### Google

1. **Issuer account.** In the Google Pay & Wallet Console, set up the Google Wallet API; note the issuer ID.
2. **Service account.** In a Google Cloud project, enable the Google Wallet API, create a service account, and create a JSON key for it. The file is the secret.
3. **Access.** In the Wallet Console, add the service account's email under Users, with developer access. While the issuer is in demo mode only the test accounts added there can save passes; request publishing access to issue to anyone, which Google reviews.
4. **Settings.** `GOOGLE_WALLET_ISSUER_ID`, `GOOGLE_WALLET_SERVICE_ACCOUNT`, and `WALLET_DELIVERY=live`; restart the api and the worker.

### Trying it on a device

1. With the settings in place and the API on the public HTTPS origin, `GET /v1/meta` names the platform.
2. **iPhone.** Make two test accounts and take an exchange to an agreement in force. Open it in Safari on the iPhone (or in a development build of the app) and press "Add to Apple Wallet": Wallet's sheet shows the pass. Add it. In the API's log, a `POST` to `/v1/wallet/apple/v1/devices/...` answered 201 is the registration.
3. As the other party, mark something delivered. Within a few seconds the worker logs `wallet passes updated`, and the pass on the iPhone says *Waiting for you* (pull down on the pass's back to make Wallet ask at once). On the device, Settings, Developer, PassKit Testing: *Additional Logging* writes Wallet's side to the console of a Mac attached to it, and *Allow HTTP Services* lets a development build reach a web service that is not HTTPS, which Wallet otherwise refuses.
4. **Android.** The same with "Add to Google Wallet" in Chrome or the app, signed in to Google as one of the issuer's test accounts while in demo mode.
5. **Revocation.** Delete the test account that holds the passes: Apple's pass turns void within a few seconds, and Google's moves to expired passes.
6. Check both languages, light and dark, and VoiceOver and TalkBack reading the pass.

## What remains

- **Not verified on a device**, because no account exists: that Wallet accepts the signature and the pass (the tests check them against the same libraries that made them, not against Apple), that APNs takes the push as sent (an empty JSON object, topic only, no `apns-push-type`, which is how Apple's pass web service documentation describes it), that Google accepts the JWT, the class and the object, and that the save link is short enough for every browser (it carries the whole object, about 2 KB).
- **iOS app.** The app opens the download link in Safari, which shows Wallet's sheet, rather than presenting the sheet itself: that needs PassKit's `PKAddPassesViewController` in a small native module (§13.1 expects one), and handing the file to the share sheet does not reliably offer Wallet. On Android the app opens the save link, which Google Wallet takes.
- **Badges.** The buttons are plain buttons with the product's own words. Apple's "Add to Apple Wallet" badge and Google's "Add to Google Wallet" button may only be used under their brand guidelines, which the owner accepts with each account; once accepted, the artwork can replace the buttons (Apple asks for its badge wherever a pass is offered).
- **The web on an Apple device** opens the same download link, so Safari adds the pass. Chrome and the others on a Mac cannot add passes and are shown no button.
- **Alias.** The pass names the other party only by an alias, and nothing sets one yet.
- **Lock-screen status.** Opt-in in §11; not offered (above).
- **Private pass type** for Google, if trying both on devices settles on it (§11).
- **Metrics.** The worker logs its Wallet passes (`wallet passes updated`, `wallet pass not updated; will retry`, `wallet pass update given up on`); it does not yet count them in `/metrics`.
