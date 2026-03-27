#import <Cocoa/Cocoa.h>

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
