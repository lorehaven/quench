---
title: API reference
parent: quench-tool
grand_parent: Crates
nav_order: 1
---

The library `quench_tool` (used by the binary and by tests). Every module is public. The functions write progress to a `&mut dyn Write` and take a `&dyn System` so tests can replace the machine. Errors are `quench_tool::error::Error` (a message string). Source in `quench-tool/src/`.

## Crate root (`lib.rs`)

```rust
pub fn run<I, T>(args: I) -> i32 where I: IntoIterator<Item = T>, T: Into<OsString>
```

Runs a command line (without the program name) with the real machine (`system::Host`), standard output, the current directory and a `watch::Poller` of 300 ms; returns the exit code.

```rust
pub fn execute<I, T>(args: I, sys: &dyn System, out: &mut dyn Write, cwd: &Path,
                     watcher: &mut dyn FnMut(&Path) -> Box<dyn Watch>) -> i32
```

As `run` on a given system, output, directory and file watcher. Parses with `Cli::try_parse_from(["quench", args...])`. Help and version go to `out` (exit 0); parse errors go to standard error (exit 2); a command error prints `error: <message>` to standard error (exit 1).

## `cli`

* `pub enum Template { Web, Fullstack }` (`ValueEnum, Clone, Copy, Debug, PartialEq, Eq`): `const fn name(self) -> &'static str` (`"web"`, `"fullstack"`), `fn from_name(name: &str) -> Option<Self>`.
* `pub struct Cli { pub cmd: Cmd }` (`Parser`; name `quench`, version from the package, about `Build quench-ui apps: new projects, a dev loop with live reload, release builds`).
* `pub enum Cmd`: `New { name, template, dir, quench_path }`, `Dev { port, open, android, linux }`, `Build { release, no_embed, bundle, target, abi }`, `Run { target, abi }`, `Linux { cmd }`, `Android { cmd }`, `Plugin { cmd }`, `Doctor`, `Test { browser, args }`. Fields as in [commands](commands.md).
* `pub enum LauncherChoice { Web, Surface }`; `pub enum BuildTarget { Apk, Aab, Linux, Deb, Rpm, Arch, Source }`; `pub enum RunTarget { Android, Linux }`.
* `pub enum LinuxCmd { Init { id, quench_path } }`; `pub enum AndroidCmd { Keystore { out, alias }, Init { package }, Launcher { which }, BuildNative { release, abi } }`; `pub enum PluginCmd { Add { name }, Update { name }, List }`.

## `error`

* `pub struct Error(pub String)` (`Debug, Clone, PartialEq, Eq`; `Display` prints the message; `From<std::io::Error>`).
* `pub type Result<T> = std::result::Result<T, Error>;`

## `project`

* `pub const MANIFEST: &str = ".quench.toml";`
* `pub struct AndroidConfig { pub package, pub version_name: Option<String>, pub version_code: Option<u32>, pub keystore, pub key_alias }` (all `Option`; `Deserialize, Clone, Debug, Default, PartialEq, Eq`).
* `pub struct LinuxConfig { pub id, pub title, pub bin, pub depends, pub wm_class, pub icon }` (all `Option<String>`).
* `pub struct Project { pub root: PathBuf, pub name: String, pub template: Template, pub android: AndroidConfig, pub linux: LinuxConfig }` with: `find(start: &Path) -> Result<Self>`, `snake() -> String` (name with `-` as `_`), `app_package()` (`<name>-app`), `server_package()` (`<name>-server`), `wasm_path(profile) -> PathBuf` (`<target>/wasm32-unknown-unknown/<profile>/<snake>_app.wasm`), `server_path(profile)` (`<target>/<profile>/<name>-server`), `workspace_root()`, `flat() -> bool`, `workspace_manifest()`, `member_path(dir) -> String`, `packages() -> Vec<String>` (names of crates in directories directly below the root, sorted), `target_dir()`, `dist()` (`<root>/dist`).
* `pub fn check_name(name: &str) -> Result<()>`: errors `the project needs a name`, `` `<name>` must start with a lowercase letter ``, `` `<name>` has `<c>` in it; use lowercase letters, digits, `-` and `_` ``, `the name is longer than 40 characters`, ``the name cannot start with `quench`: it would be taken for one of the framework's crates``.

## `scaffold`

`pub fn create(out, cwd: &Path, name: &str, template: Template, dir: Option<&str>, quench_path: Option<&str>) -> Result<Vec<PathBuf>>`: `quench new`; returns the files written.

## `templates`

* `pub fn render(name: &str, template: Template, quench_path: Option<&str>) -> Result<Vec<(String, String)>>`: path and text of every project file.
* `pub struct HostFile { pub path: String, pub bytes: Vec<u8>, pub executable: bool }`.
* `pub fn check_package(package: &str) -> Result<()>`: error ``` `<p>` is not a package name: use at least two lowercase parts joined by dots, such as com.example.app ```.
* `pub fn render_android(name: &str, package: &str) -> Result<Vec<HostFile>>`: the files of the Android host (Gradle wrapper files are copied verbatim; `gradlew` is executable).

## `versions`

`pub const CRATES: &[(&str, &str)]`, `pub const REGISTRY: &str = "ennor"`, `pub const REGISTRY_INDEX: &str`, `pub fn dependency_lines(path: Option<&str>) -> String` (see [configuration](configuration.md#pinned-versions)).

## `system`

* `pub struct Cmd { pub program: String, pub args: Vec<String>, pub cwd: PathBuf, pub env: Vec<(String, String)> }`: `new(program, args: &[&str], cwd: &Path)`, `env(self, name, value) -> Self`, `line() -> String`.
* `pub struct Http { pub method: &'static str, pub url: String, pub headers: Vec<(String, String)>, pub body: Vec<u8> }`.
* `pub trait Process { fn kill(&mut self); fn try_wait(&mut self) -> Option<i32>; }`
* `pub trait System`: `run(&self, &Cmd) -> io::Result<i32>`, `spawn(&self, &Cmd) -> io::Result<Box<dyn Process>>`, `capture(&self, program, args) -> Option<String>` (stdout then stderr when it succeeds), `var(&self, name) -> Option<String>` (empty counts as unset), `exists(&self, &Path) -> bool`, `list_dir(&self, &Path) -> Vec<String>` (sorted), `on_path(&self, program) -> bool`, `free_port(&self) -> u16`, `http(&self, &Http) -> io::Result<(u16, Vec<u8>)>` (only `PUT` and otherwise `POST`), `wait_port(&self, port, millis) -> bool` (ignores a self-connection).
* `pub struct Host` (`Debug, Default, Clone, Copy`): the real machine.

## `release`

* `pub fn step(sys, out, cmd: &Cmd) -> Result<()>`: prints `$ <line>` and runs; non-zero exit is ``` `<line>` failed (exit <n>) ```; a program that cannot start is ``` cannot run `<program>`: <e> ```.
* `pub const fn wasm_profile(release: bool) -> &'static str`: `wasm` or `wasm-dev`.
* `pub fn build(sys, out, project, release: bool, embed: bool) -> Result<()>`.
* `pub fn bundle(sys, out, project) -> Result<()>`.

## `android`

* `pub fn default_package(project) -> String` (`com.example.<snake>`); `pub fn init(out, project, package: Option<&str>) -> Result<()>`.
* `pub const ABIS: &[(&str, &str)]` = `[("arm64-v8a", "aarch64-linux-android"), ("x86_64", "x86_64-linux-android")]`.
* `pub fn ndk_home(sys) -> Result<PathBuf>`; `pub fn has_native_code(project) -> bool` (`native/Cargo.toml` or `surface/Cargo.toml` exists).
* `pub fn build_native(sys, out, project, release: bool, abi: Option<&str>) -> Result<()>`.
* `pub fn package(project) -> Result<String>`; `pub fn version(project) -> (String, u32)` (versionName, versionCode).
* `pub fn verify_apk(sys, out, apk: &Path) -> Result<()>`: runs `apksigner verify --print-certs`; errors `apksigner was not found in the SDK's build-tools: install them with sdkmanager` or `<apk> is not signed with a valid signature`.
* `pub fn build_apk(sys, out, project, release: bool, aab: bool, abi: Option<&str>) -> Result<PathBuf>`.
* `pub fn run(sys, out, project, abi: Option<&str>) -> Result<()>`; `pub fn launch_dev(sys, out, project, port: u16) -> Result<()>`.
* `pub fn keystore(sys, out, project, path: &str, alias: &str) -> Result<()>`; `pub fn launcher(out, project, which: LauncherChoice) -> Result<()>`.

## `linux`

`app_id(project) -> String`, `title(project) -> String`, `package(project) -> String` (`<name>-desktop`), `init(out, project, id: Option<&str>, quench_path: Option<&str>) -> Result<()>`, `build(sys, out, project, release_build: bool) -> Result<PathBuf>`, `run(sys, out, project) -> Result<()>`, `launch_dev(sys, out, project, port: u16) -> Result<Option<Box<dyn Process>>>`, `version(project) -> String`, `version_code(version: &str) -> u32`, `icon(project) -> Result<Option<(String, Vec<u8>)>>`, `desktop_entry(project) -> String`, `deb(sys, out, project) -> Result<PathBuf>`.

## `packaging`

`arch(sys, out, project) -> Result<PathBuf>`, `rpm(sys, out, project) -> Result<PathBuf>`, `source(out, project) -> Result<PathBuf>`. The `RELEASE` constant (`1`) is private.

## `plugins`

`names() -> Vec<&'static str>`, `installed(project) -> Vec<String>`, `list(out, project) -> Result<()>`, `add(out, project, name: &str) -> Result<()>`, `update(out, project, name: Option<&str>) -> Result<()>`, `check(name: &str) -> Result<()>`. (`ensure_dependency` is `pub(crate)`.)

## `dev`, `browser`, `watch`

* `dev::Options { pub port: u16, pub open: bool, pub android: bool, pub linux: bool }`; `dev::dev(sys, out, project, options, watch: &mut dyn Watch) -> Result<()>`.
* `browser::BROWSERS: &[&str]` (`chromium`, `chromium-browser`, `google-chrome`, `google-chrome-stable`); `browser::run(sys, out, project) -> Result<()>`.
* `watch::Event { Quiet, Changed, Stop }`; `watch::Watch { fn poll(&mut self) -> Event }`; `watch::fingerprint(root: &Path) -> u64`; `watch::Poller` with `new(root: &Path, interval: Duration)`.

## `doctor`

`MIN_RUSTC: (u32, u32) = (1, 85)`, `MIN_JAVA: u32 = 17`, `enum Status { Ok(String), Missing(String) }`, `struct Check { name: &'static str, status: Status, required: bool }`, `parse_version(text) -> Option<(u32, u32, u32)>`, `java_major(text) -> Option<u32>`, `checks(sys) -> Vec<Check>`, `report(out, checks: &[Check]) -> Result<bool>`.
