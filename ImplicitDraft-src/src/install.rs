use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(target_os = "macos")]
use anyhow::bail;
use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallStatus {
    pub target: PathBuf,
    pub installed: bool,
    pub on_path: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallResult {
    pub target: PathBuf,
    pub already_current: bool,
    pub on_path: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppInstallResult {
    pub target: PathBuf,
    pub already_current: bool,
}

pub fn current_status() -> InstallStatus {
    let target = preferred_install_path();
    let installed = env::current_exe().ok().as_ref() == Some(&target) || target.exists();
    let on_path = is_dir_on_path(target.parent().unwrap_or_else(|| Path::new(".")));

    InstallStatus {
        target,
        installed,
        on_path,
    }
}

pub fn preferred_install_path() -> PathBuf {
    match env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".local/bin/implicit"),
        None => PathBuf::from(".implicit/bin/implicit"),
    }
}

pub fn preferred_app_install_path() -> PathBuf {
    match env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join("Applications/Implicit.app"),
        None => PathBuf::from(".implicit/Applications/Implicit.app"),
    }
}

pub fn path_export_hint() -> &'static str {
    "export PATH=\"$HOME/.local/bin:$PATH\""
}

pub fn install_current_exe() -> Result<InstallResult> {
    let current = env::current_exe().context("failed to locate current executable")?;
    install_binary(&current, &preferred_install_path())
}

pub fn install_current_app_bundle() -> Result<AppInstallResult> {
    let current = env::current_exe().context("failed to locate current executable")?;
    install_app_bundle(&current, &preferred_app_install_path())
}

pub fn install_app_bundle(source_binary: &Path, target_app: &Path) -> Result<AppInstallResult> {
    let macos_dir = target_app.join("Contents/MacOS");
    let resources_dir = target_app.join("Contents/Resources");
    let target_binary = macos_dir.join("implicit");
    let target_launcher = macos_dir.join("ImplicitApp");
    let target_plist = target_app.join("Contents/Info.plist");
    let target_launcher_source = resources_dir.join("ImplicitApp.m");

    let already_current = target_binary.exists()
        && target_launcher.exists()
        && target_plist.exists()
        && target_launcher_source.exists()
        && files_match(source_binary, &target_binary)?
        && fs::read_to_string(&target_launcher_source).ok().as_deref()
            == Some(app_launcher_source())
        && fs::read_to_string(&target_plist).ok().as_deref() == Some(&app_info_plist_contents());
    if already_current {
        return Ok(AppInstallResult {
            target: target_app.to_path_buf(),
            already_current: true,
        });
    }

    fs::create_dir_all(&macos_dir)
        .with_context(|| format!("failed to create {}", macos_dir.display()))?;
    fs::create_dir_all(&resources_dir)
        .with_context(|| format!("failed to create {}", resources_dir.display()))?;

    fs::write(&target_plist, app_info_plist_contents())
        .with_context(|| format!("failed to write {}", target_plist.display()))?;
    fs::write(&target_launcher_source, app_launcher_source())
        .with_context(|| format!("failed to write {}", target_launcher_source.display()))?;

    let temp_binary = target_binary.with_extension("tmp");
    fs::copy(source_binary, &temp_binary).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source_binary.display(),
            temp_binary.display()
        )
    })?;
    fs::rename(&temp_binary, &target_binary).with_context(|| {
        format!(
            "failed to move {} to {}",
            temp_binary.display(),
            target_binary.display()
        )
    })?;
    compile_app_launcher(&target_launcher_source, &target_launcher)?;
    register_app_bundle(target_app);

    #[cfg(unix)]
    {
        fs::set_permissions(&target_binary, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to set permissions on {}", target_binary.display()))?;
    }

    Ok(AppInstallResult {
        target: target_app.to_path_buf(),
        already_current: false,
    })
}

pub fn install_binary(source: &Path, target: &Path) -> Result<InstallResult> {
    let parent = target
        .parent()
        .context("install target is missing a parent directory")?;

    if source == target {
        return Ok(InstallResult {
            target: target.to_path_buf(),
            already_current: true,
            on_path: is_dir_on_path(parent),
        });
    }

    if target.exists() && files_match(source, target)? {
        return Ok(InstallResult {
            target: target.to_path_buf(),
            already_current: true,
            on_path: is_dir_on_path(parent),
        });
    }

    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let temp_path = target.with_extension("tmp");
    fs::copy(source, &temp_path).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            temp_path.display()
        )
    })?;

    #[cfg(unix)]
    fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to set permissions on {}", temp_path.display()))?;

    fs::rename(&temp_path, target).with_context(|| {
        format!(
            "failed to move {} to {}",
            temp_path.display(),
            target.display()
        )
    })?;

    Ok(InstallResult {
        target: target.to_path_buf(),
        already_current: false,
        on_path: is_dir_on_path(parent),
    })
}

fn files_match(left: &Path, right: &Path) -> Result<bool> {
    let left_meta =
        fs::metadata(left).with_context(|| format!("failed to read {}", left.display()))?;
    let right_meta =
        fs::metadata(right).with_context(|| format!("failed to read {}", right.display()))?;
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }

    let left_bytes =
        fs::read(left).with_context(|| format!("failed to read {}", left.display()))?;
    let right_bytes =
        fs::read(right).with_context(|| format!("failed to read {}", right.display()))?;
    Ok(left_bytes == right_bytes)
}

fn is_dir_on_path(dir: &Path) -> bool {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).any(|entry| entry == dir))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn register_app_bundle(target_app: &Path) {
    const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    let _ = Command::new(LSREGISTER).arg("-f").arg(target_app).status();
}

#[cfg(not(target_os = "macos"))]
fn register_app_bundle(_target_app: &Path) {}

fn app_info_plist_contents() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>Implicit</string>
    <key>CFBundleExecutable</key>
    <string>ImplicitApp</string>
    <key>CFBundleIdentifier</key>
    <string>com.implicitdraft.implicit</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Implicit</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Markdown Document</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
            <key>LSHandlerRank</key>
            <string>Owner</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>md</string>
                <string>markdown</string>
                <string>mdown</string>
                <string>txt</string>
                <string>text</string>
            </array>
        </dict>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Code and Config Text</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>LSHandlerRank</key>
            <string>Alternate</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>rs</string>
                <string>swift</string>
                <string>swiftinterface</string>
                <string>kt</string>
                <string>kts</string>
                <string>py</string>
                <string>pyi</string>
                <string>pyw</string>
                <string>js</string>
                <string>mjs</string>
                <string>cjs</string>
                <string>ts</string>
                <string>tsx</string>
                <string>jsx</string>
                <string>go</string>
                <string>json</string>
                <string>jsonc</string>
                <string>toml</string>
                <string>yaml</string>
                <string>yml</string>
                <string>ini</string>
                <string>conf</string>
                <string>sh</string>
                <string>bash</string>
                <string>zsh</string>
                <string>fish</string>
                <string>css</string>
                <string>scss</string>
                <string>html</string>
                <string>htm</string>
                <string>xml</string>
                <string>c</string>
                <string>cc</string>
                <string>cpp</string>
                <string>cxx</string>
                <string>h</string>
                <string>hh</string>
                <string>hpp</string>
                <string>hxx</string>
                <string>ipp</string>
                <string>java</string>
                <string>rb</string>
                <string>php</string>
                <string>m</string>
                <string>mm</string>
                <string>cs</string>
                <string>scala</string>
                <string>lua</string>
                <string>dart</string>
                <string>sql</string>
                <string>r</string>
                <string>zig</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

#[cfg(target_os = "macos")]
fn compile_app_launcher(source: &Path, target: &Path) -> Result<()> {
    let temp_target = target.with_extension("tmp");
    if temp_target.exists() {
        fs::remove_file(&temp_target)
            .with_context(|| format!("failed to remove {}", temp_target.display()))?;
    }

    let status = Command::new("clang")
        .arg("-fobjc-arc")
        .arg("-framework")
        .arg("Cocoa")
        .arg("-o")
        .arg(&temp_target)
        .arg(source)
        .status()
        .context("failed to invoke clang for macOS app launcher")?;
    if !status.success() {
        bail!("failed to compile macOS app launcher");
    }

    #[cfg(unix)]
    fs::set_permissions(&temp_target, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to set permissions on {}", temp_target.display()))?;

    fs::rename(&temp_target, target).with_context(|| {
        format!(
            "failed to move {} to {}",
            temp_target.display(),
            target.display()
        )
    })?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn compile_app_launcher(_source: &Path, target: &Path) -> Result<()> {
    fs::write(target, app_launcher_fallback_script())
        .with_context(|| format!("failed to write {}", target.display()))?;

    #[cfg(unix)]
    fs::set_permissions(target, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to set permissions on {}", target.display()))?;

    Ok(())
}

fn app_launcher_source() -> &'static str {
    r#"#import <Cocoa/Cocoa.h>

@interface ImplicitAppDelegate : NSObject <NSApplicationDelegate>
@property(nonatomic, assign) BOOL launched;
@end

@implementation ImplicitAppDelegate

- (NSString *)implicitBinaryPath {
    NSString *executablePath = [[NSBundle mainBundle] executablePath];
    NSString *macosDir = [executablePath stringByDeletingLastPathComponent];
    return [macosDir stringByAppendingPathComponent:@"implicit"];
}

- (void)launchImplicitWithArguments:(NSArray<NSString *> *)arguments {
    self.launched = YES;

    NSTask *task = [[NSTask alloc] init];
    task.executableURL = [NSURL fileURLWithPath:[self implicitBinaryPath]];
    task.arguments = arguments;
    task.standardInput = [NSPipe pipe];
    task.standardOutput = [NSFileHandle fileHandleWithNullDevice];
    task.standardError = [NSFileHandle fileHandleWithNullDevice];

    NSError *error = nil;
    [task launchAndReturnError:&error];
    if (error != nil) {
        NSLog(@"Implicit launcher failed: %@", error);
    }
}

- (NSArray<NSString *> *)forwardedPaths {
    NSArray<NSString *> *arguments = [[NSProcessInfo processInfo] arguments];
    if ([arguments count] <= 1) {
        return @[];
    }

    NSMutableArray<NSString *> *paths = [NSMutableArray array];
    for (NSUInteger index = 1; index < [arguments count]; index++) {
        NSString *argument = arguments[index];
        if ([argument hasPrefix:@"-psn_"] || [argument isEqualToString:@"--app"]) {
            continue;
        }
        [paths addObject:argument];
    }
    return paths;
}

- (void)launchPaths:(NSArray<NSString *> *)paths {
    if ([paths count] == 0) {
        [self launchImplicitWithArguments:@[@"--app"]];
        return;
    }

    for (NSString *path in paths) {
        [self launchImplicitWithArguments:@[@"--app", path]];
    }
}

- (void)launchDefaultIfNeeded {
    if (!self.launched) {
        [self launchPaths:@[]];
        [NSApp terminate:nil];
    }
}

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;

    NSArray<NSString *> *paths = [self forwardedPaths];
    if ([paths count] > 0) {
        [self launchPaths:paths];
        [NSApp terminate:nil];
        return;
    }

    [self performSelector:@selector(launchDefaultIfNeeded) withObject:nil afterDelay:0.2];
}

- (void)application:(NSApplication *)sender openFiles:(NSArray<NSString *> *)filenames {
    [self launchPaths:filenames];

    [sender replyToOpenOrPrint:NSApplicationDelegateReplySuccess];
    [NSApp terminate:nil];
}

- (BOOL)applicationShouldOpenUntitledFile:(NSApplication *)sender {
    (void)sender;
    return NO;
}

@end

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        ImplicitAppDelegate *delegate = [[ImplicitAppDelegate alloc] init];
        [application setActivationPolicy:NSApplicationActivationPolicyRegular];
        [application setDelegate:delegate];
        return NSApplicationMain(argc, argv);
    }
}
"#
}

#[cfg(not(target_os = "macos"))]
fn app_launcher_fallback_script() -> &'static str {
    "#!/bin/sh\nset -eu\n\nSELF_DIR=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\"\nexec \"$SELF_DIR/implicit\" --app \"$@\"\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_nanos();
        env::temp_dir().join(format!("implicit-install-{name}-{unique}"))
    }

    #[test]
    fn install_binary_copies_source_to_target() {
        let root = temp_dir("copy");
        fs::create_dir_all(&root).expect("mkdir");
        let source = root.join("source-bin");
        let target = root.join("bin/implicit");
        fs::write(&source, "binary").expect("seed");

        let result = install_binary(&source, &target).expect("install");

        assert_eq!(result.target, target);
        assert_eq!(fs::read_to_string(result.target).expect("target"), "binary");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn install_binary_marks_identical_target_as_current() {
        let root = temp_dir("current");
        fs::create_dir_all(root.join("bin")).expect("mkdir");
        let source = root.join("source-bin");
        let target = root.join("bin/implicit");
        fs::write(&source, "binary").expect("seed source");
        fs::write(&target, "binary").expect("seed target");

        let result = install_binary(&source, &target).expect("install");

        assert!(result.already_current);
        assert_eq!(fs::read_to_string(&target).expect("target"), "binary");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn install_app_bundle_creates_bundle_contents() {
        let root = temp_dir("app-bundle");
        fs::create_dir_all(&root).expect("mkdir");
        let source = root.join("implicit-bin");
        let target = root.join("Applications/Implicit.app");
        fs::write(&source, "binary").expect("seed");

        let result = install_app_bundle(&source, &target).expect("install app bundle");

        assert_eq!(result.target, target);
        assert!(target.join("Contents/Info.plist").exists());
        assert!(target.join("Contents/MacOS/ImplicitApp").exists());
        assert!(target.join("Contents/MacOS/implicit").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }
}
