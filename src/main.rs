#![allow(non_snake_case)]
use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyAccessory,
    NSWindow, NSWindowStyleMask, NSWindowCollectionBehavior,
    NSBackingStoreType,
};
use objc::runtime::Class;
use cocoa::base::nil;
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize};

use std::ffi::c_void;
use std::thread;

// CGS private APIs for space management
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGSDefaultConnectionForThread() -> CGSConnection;
    fn CGSGetActiveSpace(conn: CGSConnection) -> i32;
    fn CGSMoveWindowsToManagedSpace(
        conn: CGSConnection,
        window_ids: *const c_void,
        space: i32,
    ) -> i32;
}
type CGSConnection = *mut c_void;

// Window level constants - using very high level to stay above ALL windows
// Standard levels: Normal=0, Floating=5, Dock=20, ScreenSaver=1000, Overlay=102
// Using a very high value to ensure we're always on top
const kCGFloatingWindowLevel: i64 = 2147483631; // CGWindowLevelForKey(kCGMaximumWindowLevelKey) - near max

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
        
        // Set collection behavior to follow all spaces
        let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoresCycle;
        window.setCollectionBehavior_(behavior);
        
        window.center();
        window.makeKeyAndOrderFront_(nil);

        // Get window number using message passing
        let window_number: i64 = msg_send![window, windowNumber];
        
        // Desktop-jump thread - follows user to each space
        thread::spawn(move || {
            let conn = CGSDefaultConnectionForThread();
            let mut last_space = CGSGetActiveSpace(conn);
            loop {
                thread::sleep(std::time::Duration::from_millis(500));
                let curr_space = CGSGetActiveSpace(conn);
                if curr_space != last_space && curr_space != 0 {
                    last_space = curr_space;
                    let ids: [i64; 1] = [window_number];
                    CGSMoveWindowsToManagedSpace(
                        conn,
                        ids.as_ptr() as *const c_void,
                        curr_space,
                    );
                }
            }
        });

        app.run();
    }
}

#[macro_use]
extern crate objc;
