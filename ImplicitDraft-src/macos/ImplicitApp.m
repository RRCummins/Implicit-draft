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

- (NSArray<NSString *> *)forwardedArguments {
    NSArray<NSString *> *arguments = [[NSProcessInfo processInfo] arguments];
    if ([arguments count] <= 1) {
        return @[];
    }

    NSMutableArray<NSString *> *forwarded = [NSMutableArray arrayWithObject:@"--app"];
    for (NSUInteger index = 1; index < [arguments count]; index++) {
        NSString *argument = arguments[index];
        if ([argument hasPrefix:@"-psn_"]) {
            continue;
        }
        [forwarded addObject:argument];
    }
    return forwarded;
}

- (void)launchDefaultIfNeeded {
    if (!self.launched) {
        [self launchImplicitWithArguments:@[@"--app"]];
        [NSApp terminate:nil];
    }
}

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;

    NSArray<NSString *> *forwarded = [self forwardedArguments];
    if ([forwarded count] > 1) {
        [self launchImplicitWithArguments:forwarded];
        [NSApp terminate:nil];
        return;
    }

    [self performSelector:@selector(launchDefaultIfNeeded) withObject:nil afterDelay:0.2];
}

- (void)application:(NSApplication *)sender openFiles:(NSArray<NSString *> *)filenames {
    if ([filenames count] == 0) {
        [self launchDefaultIfNeeded];
    } else {
        for (NSString *filename in filenames) {
            [self launchImplicitWithArguments:@[@"--app", filename]];
        }
    }

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
