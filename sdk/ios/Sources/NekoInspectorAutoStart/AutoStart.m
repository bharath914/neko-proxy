#if defined(__APPLE__)
#import <Foundation/Foundation.h>
#import "NekoInspectorAutoStart.h"

@interface NekoInspectorAutoStartLoader : NSObject
@end

@implementation NekoInspectorAutoStartLoader
+ (void)load {
    neko_inspector_autostart();
}
@end
#endif
