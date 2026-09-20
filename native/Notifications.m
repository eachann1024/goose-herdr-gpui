#import <AppKit/AppKit.h>
#import <UserNotifications/UserNotifications.h>

typedef void (*PermissionCallback)(int);
typedef void (*RouteCallback)(const char *, const char *);
static RouteCallback routeCallback;

bool goose_accessibility_reduce_motion(void) {
    return NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion;
}

@interface GooseNotificationDelegate : NSObject <UNUserNotificationCenterDelegate>
@end
@implementation GooseNotificationDelegate
- (void)userNotificationCenter:(UNUserNotificationCenter *)center willPresentNotification:(UNNotification *)notification withCompletionHandler:(void (^)(UNNotificationPresentationOptions))completion {
    completion(UNNotificationPresentationOptionBanner);
}
- (void)userNotificationCenter:(UNUserNotificationCenter *)center didReceiveNotificationResponse:(UNNotificationResponse *)response withCompletionHandler:(void (^)(void))completion {
    NSDictionary *info = response.notification.request.content.userInfo;
    NSString *device = info[@"deviceID"], *pane = info[@"paneID"];
    if ([device isKindOfClass:NSString.class] && [pane isKindOfClass:NSString.class]) {
        dispatch_async(dispatch_get_main_queue(), ^{
            [NSApp activateIgnoringOtherApps:YES];
            if (routeCallback) routeCallback(device.UTF8String, pane.UTF8String);
        });
    }
    completion();
}
@end
static GooseNotificationDelegate *notificationDelegate;
static UNUserNotificationCenter *center(void) {
    // UNUserNotificationCenter raises on an unbundled executable; don't imply authorization.
    if (!NSBundle.mainBundle.bundleIdentifier) return nil;
    return UNUserNotificationCenter.currentNotificationCenter;
}
void goose_notifications_init(RouteCallback callback) {
    routeCallback = callback;
    dispatch_async(dispatch_get_main_queue(), ^{
        notificationDelegate = [GooseNotificationDelegate new];
        center().delegate = notificationDelegate;
    });
}
void goose_notifications_permission(bool request, PermissionCallback callback) {
    UNUserNotificationCenter *c = center();
    if (!c) { callback(-1); return; }
    if (request) {
        [c requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound) completionHandler:^(BOOL granted, NSError *error) {
            callback(error ? -1 : granted ? 2 : 1);
        }];
    } else {
        [c getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings *settings) {
            callback((int)settings.authorizationStatus);
        }];
    }
}
void goose_notifications_post(const char *title, const char *body, const char *device, const char *pane) {
    UNUserNotificationCenter *c = center(); if (!c) return;
    UNMutableNotificationContent *content = [UNMutableNotificationContent new];
    content.title = [NSString stringWithUTF8String:title]; content.body = [NSString stringWithUTF8String:body];
    NSString *deviceID = [NSString stringWithUTF8String:device], *paneID = [NSString stringWithUTF8String:pane];
    content.userInfo = @{ @"deviceID": deviceID, @"paneID": paneID };
    NSString *identifier = [NSString stringWithFormat:@"agent-%@-%@", deviceID, paneID];
    [c addNotificationRequest:[UNNotificationRequest requestWithIdentifier:identifier content:content trigger:nil] withCompletionHandler:nil];
}
void goose_notifications_sound(bool blocked) {
    dispatch_async(dispatch_get_main_queue(), ^{ [[NSSound soundNamed:blocked ? @"Funk" : @"Glass"] play]; });
}
