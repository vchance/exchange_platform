# Releasing the mobile app

What is in place for building the Yuppers app for iOS and Android, what the owner does once the Expo, Apple and Google accounts exist, and the facts the store forms ask for. Nothing here needs an account until "Once the accounts exist"; nothing here has been built, signed or submitted.

Everything below describes what the code does as of this writing. It is not legal advice and makes no legal claim; where a store form asks for a judgement, the facts are here and the judgement is the owner's.

## Builds

`apps/mobile/eas.json` has four build profiles:

| Profile | What it is | Who can install it |
|---|---|---|
| `development` | A development client (`expo-dev-client`): the app's native code with a launcher that loads the JavaScript from a development server. | Internal distribution: an Android APK anyone can install; on iOS, devices registered to the Apple team (ad hoc). |
| `development-simulator` | The same, built for the iOS Simulator. | Anyone with Xcode's Simulator. Needs no Apple Developer account. |
| `preview` | A release build of the app as it is, for testing before a store. Android builds an APK. | Internal distribution, as above. |
| `production` | The build for the App Store and Google Play. | The stores. |

Each profile names an EAS environment (`development`, `preview`, `production`), and the app's two build-time settings, `EXPO_PUBLIC_API_URL` and `EXPO_PUBLIC_WEB_URL`, are kept there as EAS environment variables rather than in the repository. A build with neither set talks to `http://localhost:8080`, which only a simulator on the development machine can reach.

**Version numbers.** Two numbers identify a build. The version people see, `0.1.0` today, is `version` in `apps/mobile/app.json` and is changed by hand, in a commit, when a release means something new. The build number (iOS `CFBundleVersion`, Android `versionCode`), which each store requires to go up with every upload, is kept by EAS (`"appVersionSource": "remote"`) and raised by one for every `production` build (`"autoIncrement": true`). It is not in the app config, so no build has to write a change back to the repository, and two builds started at once cannot take the same number. Development and preview builds use whatever the remote number is at the time.

The version is also what the app reports to the service in `X-Client-Version` (`ios/0.1.0`), and what `MIN_CLIENT_VERSION_IOS` and `MIN_CLIENT_VERSION_ANDROID` compare against (README, "A client that is too old"). It was `0.0.0` before: a real build would have reported a version below any minimum a deployment set.

**Export compliance.** `ios.config.usesNonExemptEncryption` is `false`, so App Store Connect does not ask about encryption at each upload. The app's own code uses no encryption beyond what the operating system provides: HTTPS to the service, and the Keychain or Keystore through `expo-secure-store`. `expo-crypto` is used only to make random identifiers. Confirm this answer is right for the release before the first upload.

## Launch screen

The Yuppers mark (`assets/splash-icon.png`, drawn by `scripts/make-icons.mjs`) on the app's background: white in light mode, `#0f1216` in dark, the `background` colors of `src/lib/theme.ts`. It is configured with the `expo-splash-screen` plugin in `app.json`. It stays up until the app has asked the service who is signed in, so the first screen people see is the real one, and goes after four seconds in any case, so a slow network never leaves someone looking at a logo (`src/lib/splash.ts`). The window behind every screen is painted the same background with `expo-system-ui`, so nothing white shows between the launch screen and the first screen, or behind a screen sliding in, in dark mode.

## Permissions

What each platform's generated project asks for, read from `Info.plist` and `AndroidManifest.xml` produced by `npx expo prebuild` with the app config as it is now, and from the Android manifests of the dependencies that are merged into it at build time.

**iOS.** No permission prompt. The app asks for no camera, photos, contacts, location, microphone, notifications or tracking. `expo-secure-store`'s Face ID usage string is turned off (`faceIDPermission: false`): the session token is kept in the Keychain without biometrics. The only usage string in `Info.plist` is `NSLocalNetworkUsageDescription`, with the Bonjour service `_expo._tcp`, added by the development client so it can find a development server on the local network; a build step added by the same plugin deletes both from every build that is not Debug, so preview and production builds carry neither.

**Android.**

| Permission | Where it comes from | Kept? |
|---|---|---|
| `INTERNET` | The app template, `expo-file-system` | Kept: the app talks to the service. Granted at install, never prompted. |
| `READ_EXTERNAL_STORAGE`, `WRITE_EXTERNAL_STORAGE` (up to Android 12) | The app template, `expo-file-system` | Removed. The only file the app writes is a copy of a record for the share sheet, in its own cache directory, which needs no permission. |
| `SYSTEM_ALERT_WINDOW` | The app template (React Native's debug overlay) | Removed. The app draws over no other app. |
| `VIBRATE` | The app template | Removed. Nothing in the app vibrates. |

They are removed with `android.blockedPermissions` in `app.json`, which marks each `tools:node="remove"` so that a dependency's manifest cannot bring it back at build time. CI generates both projects on every change and fails if any of the four reappears, or if an iOS usage string other than the development client's appears ("Checks in CI", below).

No permission string is shown to anyone, so none needed translating. If one is added, `app.json` takes per-language strings through Expo's `locales` setting, and it should have English and Spanish.

## iOS privacy manifest

Apple requires an app to declare why it uses certain APIs ("required reason APIs") and what data it collects, in a `PrivacyInfo.xcprivacy` file. Expo writes it from `ios.privacyManifests` in `app.json`. React Native and several Expo packages ship manifests of their own, but Apple does not reliably read the manifests of static CocoaPods libraries, which is how they are linked, so the app's manifest repeats every reason they declare (Expo's guide, "Privacy manifests").

**Tracking:** none (`NSPrivacyTracking` false, no tracking domains). Nothing in the app or its dependencies tracks people across apps or websites, and there is no advertising or analytics SDK.

**Required reason APIs**, from the manifests in `node_modules`:

| API category | Reason | Declared by |
|---|---|---|
| User defaults | `CA92.1`: reading and writing the app's own settings | React Native, `expo-constants`, `expo-localization`, `expo-system-ui`; `expo-sharing` also reads its own defaults |
| File timestamps | `C617.1`: timestamps of files in the app's own container | React Native, its Folly, boost and glog libraries, `expo-application` |
| File timestamps | `0A2A.1`: a library's own file functions, used by the app; `3B52.1`: files the person chose to give the app | `expo-file-system` |
| Disk space | `E174.1`: checking there is room before writing; `85F4.1`: showing free space | `expo-file-system` |
| System boot time | `35F9.1`: measuring time elapsed within the app | React Native's timing code, boost |

The other native dependencies (`expo-secure-store`, `expo-print`, `expo-clipboard`, `expo-crypto`, `expo-splash-screen`, `expo-linking`, `expo-router`, the date picker, `react-native-screens`, `react-native-safe-area-context`) ship no manifest, and a search of their iOS sources found none of these APIs. Run the same search again after adding or upgrading a native dependency:

```sh
find node_modules -name PrivacyInfo.xcprivacy -not -path '*template*'
```

**Collected data** (`NSPrivacyCollectedDataTypes`): email address, phone number, name, user ID, and other user content, each linked to the person, none used for tracking, each for app functionality. The next section says what each is.

## What the app collects, for the store forms

For the App Store privacy label ("App Privacy" in App Store Connect) and the Google Play data-safety form. Both ask about data that leaves the device; this is what the app sends to the service, and what the service keeps.

| Data | What it is in the code | Linked to the person | Purpose |
|---|---|---|---|
| Email address | The address a person signs in with; a one-time code is sent to it. Kept on the account once verified. | Yes | Signing in; notification emails about their yups |
| Phone number | Accepted as a sign-in identifier and kept on the account once verified. Codes are not yet sent by SMS (README, "Not built yet"). | Yes | Signing in |
| Name | The display name a person gives their profile; it appears on their agreements and in the other party's copy of the record. | Yes | App functionality |
| User ID | The account's identifier, made by the service. | Yes | App functionality |
| Other user content | The agreement's text (what each party will give, when, and how they will know it is done), each revision, delivery claims and confirmations, and the reason given in a report. | Yes | App functionality; reports are kept for moderation |

Also true, and relevant to how the owner answers some questions:

- **Network address and user agent.** When someone signs an agreement, the service records the request's IP address and user agent with the signature, and deletes them after 90 days (`acceptance_network_metadata`, `backend/src/exchanges/service.rs`). For sign-in limits it keeps counts keyed by a keyed hash of the address, never the address itself (`sign_in_limit`). Neither is used to work out a location.
- **Age.** A person confirms they are 18 or over before signing; the service keeps the time they confirmed, not a date of birth.
- **Language.** The account's language preference.
- **Diagnostics:** none. There is no crash reporting, analytics or performance SDK in the app. The service logs each request's method, path, status and timing, with nothing personal (README, "Deploying").
- **Device identifiers:** none. The app sends its platform and version (`X-Client-Version: ios/0.1.0`), which identifies the build, not the device. It does not read the advertising identifier or any other device ID.
- **On the device.** The session token is kept in the Keychain or Keystore and nowhere else. An invitation's token is held in memory only. A copy of a record made for the share sheet is written to the app's cache and deleted when the next copy is made, on sign-out, and on iOS when the sheet closes. On Android, secure storage is excluded from device backups (`expo-secure-store`'s backup rules).
- **Shared with others.** The other party to a yup sees what the agreement and its record contain, including the person's name. Emails go through the SMTP provider a deployment configures. Nothing is sold or sent to advertisers or data brokers.
- **In transit.** The app talks to the service over whatever `EXPO_PUBLIC_API_URL` names; iOS refuses plain HTTP except to the local network, so a production build must use HTTPS.
- **Deletion.** A person can delete their account inside the app (account screen) and on the web (README, "Deleting an account"; `backend/src/deletion.rs` says exactly what goes and what stays).
- **Payments:** none. The app never handles money; any payment between the parties happens outside it.

## Universal links and app links

Two kinds of link are claimed (`DESIGN.md` §4.1): an invitation link, `https://<domain>/{language}/i#<token>`, and an exchange's own pages, `https://<domain>/exchanges/<id>` and `https://<domain>/exchanges/<id>/record`, which is where notification emails link to. With the app installed, iOS and Android can open such a link in the app instead of the browser, once three things agree:

1. **The app names the domain.** `apps/mobile/app.config.ts` takes the host of `EXPO_PUBLIC_WEB_URL` at build time, the same setting invitation links are written with, and adds `applinks:<domain>` to the iOS associated domains and a verified (`autoVerify`) Android intent filter for `https://<domain>/*/i` and every path under `https://<domain>/exchanges/`. Only an HTTPS origin on the default port counts; a development build against `http://localhost:5173` names no domain and keeps using `yuppers://` links.
2. **The domain names the app.** The API serves `/.well-known/apple-app-site-association` and `/.well-known/assetlinks.json` as JSON, with no redirect, from its settings, and serves neither until they are set (`backend/src/http/web.rs`, `AppLinks`):

   | Setting | Value |
   |---|---|
   | `APPLE_APP_ID` | `<Team ID>.app.yuppers`, such as `ABCDE12345.app.yuppers`. Several may be listed, separated by commas. |
   | `ANDROID_SHA256_CERT_FINGERPRINTS` | The SHA-256 fingerprints of the certificates the Android app is signed with, as colon-separated hex pairs, separated by commas. |
   | `ANDROID_PACKAGE` | Optional; `app.yuppers` by default. |

   The iOS file lists the paths the app takes (`/*/i`, `/*/i/` and `/exchanges/*`); the Android file only names the app, and the paths are in the app's intent filter. A value that is not one stops the API at start. The files must be served from the web origin itself, so this works as it is when the API serves the web app (`WEB_DIR`, the container image's default); a deployment that serves the web app from somewhere else must serve the two files there.
3. **The link reaches the right screen.** Every link the system hands the app goes through `src/app/+native-intent.ts` (`routeForIncomingLink` in `src/lib/invitation.ts`). An invitation link gives up its token, which is held in memory, and opens the invitation screen, for the `https` link exactly as for `yuppers://`. An exchange's address opens that exchange's screen, or its record; someone signed out is asked to sign in first and then sees it. An address under `/exchanges/` that is neither (a page emails never link to) is handed to the router as it came. The tests cover the `https` forms; no device has opened one yet.

Every other page of the web app stays in the browser. A deployment whose files were cached by Apple before `/exchanges/*` was added picks it up when Apple's servers fetch the file again, which can take a day or more; Android checks again when the app is installed or updated.

## Once the accounts exist

In this order. Each step needs only what the steps before it set up.

1. **Expo account and project.** Create the account, then in `apps/mobile`:

   ```sh
   npx eas-cli@latest login
   npx eas-cli@latest init
   ```

   `eas init` creates the project and gives it an ID. Because the app config is `app.config.ts`, it cannot write the ID itself and prints it instead: add it to `app.json` as `"extra": { "eas": { "projectId": "<id>" } }`, with `"owner": "<account>"` beside `"slug"`, and commit both. The repository has neither yet because both need the account.
2. **Build settings.** Create the EAS environment variables for each environment, `development`, `preview` and `production`: `EXPO_PUBLIC_API_URL` (the service) and `EXPO_PUBLIC_WEB_URL` (the web origin, `https://<domain>`), with `npx eas-cli@latest env:create`, or on expo.dev.
3. **First builds, without Apple or Google.** `eas build --profile development-simulator --platform ios` for the Simulator, and `eas build --profile development --platform android` or `--profile preview` for an APK to install on any Android phone; EAS makes and keeps the Android upload key. This is the first time the app runs on a device: go through sign-in, an invitation link, signing, delivery, the record and its share sheet, report and block, deletion, both languages, light and dark, and large text, on both platforms.
4. **Apple Developer account.** Note the Team ID (Membership details). Register the bundle ID `app.yuppers`; `eas build` does this, and turns on the Associated Domains capability the config asks for, when it first signs an iOS build. Set `APPLE_APP_ID=<Team ID>.app.yuppers` on the API. Device builds of `development` and `preview` then work for registered devices (`eas device:create`).
5. **Google Play developer account.** Create the app with the package `app.yuppers` and use Play App Signing. Copy the SHA-256 fingerprint of the app signing key (Play Console, Test and release, App integrity), and of the upload key that EAS keeps (`eas credentials`, Android); set both on the API as `ANDROID_SHA256_CERT_FINGERPRINTS`. Builds installed from Play are signed with the first, internal builds with the second.
6. **The domain.** With the API serving `https://<domain>` (`WEB_ORIGIN`) and both settings above, check:

   ```sh
   curl -i https://<domain>/.well-known/apple-app-site-association   # 200, application/json, no redirect
   curl -i https://<domain>/.well-known/assetlinks.json
   curl -s https://app-site-association.cdn-apple.com/a/v1/<domain>  # Apple's copy; can take a while to refresh
   ```

   Google's checker: `https://digitalassetlinks.googleapis.com/v1/statements:list?source.web.site=https://<domain>&relation=delegate_permission/common.handle_all_urls`. On an Android device with a build installed: `adb shell pm verify-app-links --re-verify app.yuppers`, then `adb shell pm get-app-links app.yuppers` should show the domain as `verified`. On iOS, tap an invitation link in Notes or Messages (not typed into Safari, which never opens an app).
7. **Store listings.** Create the App Store Connect record and the Play Console listing from the drafts below; fill the App Privacy label and the data-safety form from "What the app collects"; answer the content rating questionnaires (the app has user content, reporting and blocking, and is for people 18 and over); publish the privacy policy and support pages and put their addresses in both stores.
8. **Production builds.** `eas build --profile production --platform all`. The build number starts at 1 and goes up by one each time; `eas build:version:set` sets it if a store already has a higher one. Upload through each store's own console or `eas submit`, when the owner decides to.

## Store listing drafts

Drafts for the owner to edit. Lengths are counted in characters and are within each store's limit. The word "yup" is not translated (`DESIGN.md` §1.1).

### English

- **App name:** Yuppers
- **Subtitle** (App Store, 30): Send a yup. Get it in writing.
- **Short description** (Google Play, 80): Send a yup: a two-person agreement, signed and tracked. It never moves money.
- **Keywords** (App Store, 100): agreement,deal,trade,swap,promise,sign,record,handshake,deposit,job,service,barter,IOU,contract
- **Category:** Productivity (App Store and Google Play); Business as the App Store's secondary category.
- **Support URL:** `https://<domain>/support` (placeholder; no such page exists yet)
- **Privacy policy URL:** `https://<domain>/privacy` (placeholder; no such page exists yet)

**Full description:**

> Said yes to a deal? Send a yup.
>
> A yup is a two-person agreement, signed and tracked. Write down what each of you will give (a job, an item, a favor, or money paid your own way) and when it's due. Send it with a link. The other person reads it, suggests changes or signs. Once you've both signed, Yuppers keeps track of who has delivered what, until it's done.
>
> What it does
> • Write the terms together. Every version is kept.
> • Invite the other person with a link. They can read it in a browser, without the app.
> • Sign in with a one-time code sent to your email. No password.
> • Sign the agreement electronically, both of you.
> • Mark what you delivered and confirm what you received.
> • Keep the whole history, and save the record as a PDF or share it.
> • Report or block someone, right from the yup.
> • Delete your account from inside the app.
> • In English and Spanish, in light and dark.
>
> What it doesn't do
> • It doesn't move money. Any payment happens between you, outside the app.
> • It doesn't decide disputes or say who's right. It keeps a shared record of what each of you says was done.
> • It doesn't find people for you. It's for deals you've already decided to make.
>
> For people 18 and over.

### Español

- **Nombre:** Yuppers
- **Subtítulo** (App Store, 30): Manda un yup, queda escrito
- **Descripción breve** (Google Play, 80): Manda un yup: un acuerdo entre dos, firmado y seguido. Nunca mueve dinero.
- **Palabras clave** (App Store, 100): acuerdo,trato,intercambio,trueque,firma,registro,promesa,servicio,encargo,contrato,compromiso
- **Categoría:** Productividad (App Store y Google Play); Negocios como categoría secundaria en el App Store.
- **URL de soporte:** `https://<domain>/es/support` (provisional; la página aún no existe)
- **URL de la política de privacidad:** `https://<domain>/es/privacy` (provisional; la página aún no existe)

**Descripción completa:**

> ¿Ya cerraron el trato? Manda un yup.
>
> Un yup es un acuerdo entre dos personas, firmado y con seguimiento. Anoten lo que cada uno va a dar (un trabajo, un objeto, un favor o un dinero que se paga por fuera) y para cuándo. Mándalo con un enlace. La otra persona lo lee, propone cambios o firma. Cuando los dos han firmado, Yuppers lleva la cuenta de quién entregó qué, hasta que todo esté hecho.
>
> Lo que hace
> • Escriban los términos juntos. Se guarda cada versión.
> • Invita a la otra persona con un enlace. Puede leerlo en el navegador, sin la app.
> • Entra con un código de un solo uso que llega a tu correo. Sin contraseña.
> • Firmen el acuerdo electrónicamente, los dos.
> • Marca lo que entregaste y confirma lo que recibiste.
> • Guarda todo el historial, y guarda el registro como PDF o compártelo.
> • Denuncia o bloquea a alguien desde el mismo yup.
> • Elimina tu cuenta desde la app.
> • En español e inglés, en modo claro y oscuro.
>
> Lo que no hace
> • No mueve dinero. Cualquier pago es entre ustedes, fuera de la app.
> • No resuelve disputas ni dice quién tiene razón. Guarda un registro compartido de lo que cada uno dice que se hizo.
> • No te busca con quién tratar. Es para tratos que ya decidieron hacer.
>
> Para mayores de 18 años.

## Screenshot plan

Taken from development or preview builds against a service with made-up people (`example.test` addresses, names such as Ana Ruiz and Ben Ortiz, as the tests use), never real accounts. The same set in English and in Spanish, light mode, the device's default text size; one dark-mode shot at the end of each set.

| # | Screen | State |
|---|---|---|
| 1 | The exchanges list | Signed in as Ana: two active yups (one waiting on the other person, one with a delivery due) and closed ones folded away |
| 2 | An invitation, opened from its link | Ben reading Ana's terms before signing up, with the notice that money is paid outside the app |
| 3 | Composing terms | Ana's yup with one item each side and a due date |
| 4 | Signing | The consent and the button to sign, both names shown |
| 5 | The yup after signing | One delivery marked by Ana and waiting for Ben's confirmation |
| 6 | The history | Proposal, a counter-offer, both signatures and the delivery |
| 7 | The record | The plain summary at the top, with the share sheet open on "Save as PDF" |
| 8 | Report and block | The panel open from the yup |
| 9 | Any of the above in dark mode | |

Sizes each store asks for at the time of writing (check before taking them):

- **App Store:** iPhone 6.9-inch display, 1320 × 2868 or 1290 × 2796 portrait, three to ten shots. Because the app runs on iPad (`supportsTablet: true`), iPad 13-inch shots too, 2064 × 2752 or 2048 × 2732. App Store Connect scales these for smaller devices.
- **Google Play:** at least two and up to eight phone screenshots, PNG or JPEG, 16:9 or 9:16, each side between 320 and 3840 pixels (1080 × 1920 is a safe choice); a 1024 × 500 feature graphic; a 512 × 512 icon, which `scripts/make-icons.mjs` does not write yet (one more `png(...)` line there, from the same mark). Tablet screenshots are optional.

## Checks in CI

The "Mobile app config" job (`.github/workflows/ci.yml`) runs on every change:

- `npx expo-doctor`, Expo's checks of the dependencies against the SDK and of the app config;
- `npx expo prebuild --no-install` for iOS and for Android, with `EXPO_PUBLIC_WEB_URL=https://yuppers.example`, in a copy of the app outside the checkout that is deleted afterwards; then checks that the generated projects carry the associated domain, the verified Android intent filter, the privacy manifest and the four removed permissions, and no iOS usage string beyond the development client's.

It builds and signs nothing and needs no account. The generated `ios/` and `android/` folders are never committed (`apps/mobile/.gitignore`); `app.json` and `app.config.ts` are the source of truth.

## Not verified

- Nothing here has run on a device or a simulator, or been built by EAS. `npx expo prebuild`, `npx expo export` for both platforms, `expo-doctor` and the jest tests (as iOS and as Android) have.
- Whether iOS hands the app the link's `#token` fragment intact for a universal link, and Android for an app link, is what the system documentation says and what `+native-intent.ts` is written for; it has not been seen on a device.
- The privacy manifest reasons come from the dependencies' own manifests and a search of their sources; Apple's check at upload is the real test.
