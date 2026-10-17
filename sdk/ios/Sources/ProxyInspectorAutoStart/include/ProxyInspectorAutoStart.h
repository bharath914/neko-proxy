// Linking the ProxyInspectorAutoStart product starts the SDK from +load.
// Caveat (unverified, needs Xcode): a static library's +load can be dropped if nothing references the object file.
// If the SDK does not start on its own, add `-ObjC` to Other Linker Flags, or call Inspector.start().
void proxy_inspector_autostart(void);
