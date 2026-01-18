#![allow(non_snake_case)]
use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyAccessory,
    NSWindow, NSWindowStyleMask, NSWindowCollectionBehavior,
    NSBackingStoreType,
};
use objc::runtime::Class;
use cocoa::base::nil;
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize};

#[allow(non_upper_case_globals)]
const kCGFloatingWindowLevel: i64 = 2147483631;

// NSWindowSharingType values
#[allow(non_upper_case_globals)]
const NSWindowSharingNone: u64 = 0; // Window is not visible in screen capture

fn main() {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

        let frame = NSRect::new(
            NSPoint::new(200.0, 200.0),
            NSSize::new(400.0, 300.0),
        );

        let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            frame,
            NSWindowStyleMask::NSBorderlessWindowMask,
            NSBackingStoreType::NSBackingStoreBuffered,
            false,
        );

        let ns_color_class = Class::get("NSColor").unwrap();
        let black_color: cocoa::base::id = msg_send![ns_color_class, blackColor];
        window.setBackgroundColor_(black_color);
        window.setOpaque_(true);
        window.setHasShadow_(false);
        window.setLevel_(kCGFloatingWindowLevel);
        
        // Make window invisible in screen recording and screen sharing
        let _: () = msg_send![window, setSharingType: NSWindowSharingNone];
        
        // Set collection behavior to appear on all spaces
        let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;
        window.setCollectionBehavior_(behavior);
        
        window.center();
        window.makeKeyAndOrderFront_(nil);

        app.run();
    }
}

#[macro_use]
extern crate objc;
