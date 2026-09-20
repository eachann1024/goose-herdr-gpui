#import <Foundation/Foundation.h>
#import <Tailcat/Tailcat.h>

int herdr_tailcat_start(const char *token, const char *path) {
    @autoreleasepool {
        NSError *error = nil;
        return TailcatmobileStartBridge([NSString stringWithUTF8String:token],
                                       [NSString stringWithUTF8String:path], &error) ? 1 : 0;
    }
}

int herdr_tailcat_has_error(const char *path) {
    @autoreleasepool {
        return TailcatmobileBridgeError([NSString stringWithUTF8String:path]).length > 0;
    }
}

void herdr_tailcat_stop(const char *path) {
    @autoreleasepool {
        TailcatmobileStopBridge([NSString stringWithUTF8String:path]);
    }
}
