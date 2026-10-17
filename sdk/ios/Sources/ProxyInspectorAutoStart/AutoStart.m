#if defined(__APPLE__)
#import <Foundation/Foundation.h>
#import "ProxyInspectorAutoStart.h"

@interface ProxyInspectorAutoStartLoader : NSObject
@end

@implementation ProxyInspectorAutoStartLoader
+ (void)load {
    proxy_inspector_autostart();
}
@end
#endif
