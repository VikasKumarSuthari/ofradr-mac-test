#![allow(non_snake_case)]
use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyAccessory, NSBorderlessWindowMask,
    NSColor, NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehaviorCanJoinAllSpaces,
    NSWindowCollectionBehaviorStationary, NSWindowCollectionBehaviorIgnoresCycle,
};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use core_graphics::display::{CGDisplayBounds, CGMainDisplayID};
use objc::runtime::{Object, Sel};
use std::ffi::c_void;
use std::thread;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGSGetSpaceID(conn: CGSConnection, pid: i32) -> i32;
    fn CGSDefaultConnectionForThread() -> CGSConnection;
    fn CGSMoveWindowsToManagedSpace(
        conn: CGSConnection,
        window_ids: *const c_void,
        space: i32,
    ) -> i32;
}
type CGSConnection = *mut c_void;

fn main() {
    unsafe {
        let _pool = cocoa::foundation::NSAutoreleasePool::new(nil);

        NSApp().setActivationPolicy_(NSApplicationActivationPolicyAccessory);

        let frame = {
            let display = CGMainDisplayID();
            let bounds = CGDisplayBounds(display);
            NSRect::new(
                NSPoint::new(200.0, 200.0),
                NSSize::new(400.0, 300.0),
            )
        };

        let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            frame,
            NSBorderlessWindowMask,
            cocoa::appkit::NSBackingStoreBuffered,
            false,
        );

        window.setBackgroundColor_(NSColor::clearColor(nil));
        window.setOpaque_(false);
        window.setHasShadow_(false);
        window.setLevel_(NSFloatingWindowLevel);
        window.setCollectionBehavior_(
            NSWindowCollectionBehaviorCanJoinAllSpaces
                | NSWindowCollectionBehaviorStationary
                | NSWindowCollectionBehaviorIgnoresCycle,
        );
        window.center();
        window.makeKeyAndOrderFront_(nil);

        // desktop-jump thread
        let window_id = window.windowNumber();
        thread::spawn(move || unsafe {
            let conn = CGSDefaultConnectionForThread();
            let mut last = CGSGetSpaceID(conn, std::process::id() as i32);
            loop {
                thread::sleep(std::time::Duration::from_millis(500));
                let curr = CGSGetSpaceID(conn, std::process::id() as i32);
                if curr != last {
                    last = curr;
                    let ids = vec![window_id];
                    CGSMoveWindowsToManagedSpace(
                        conn,
                        ids.as_ptr() as *const c_void,
                        curr,
                    );
                }
            }
        });

        let app = NSApp();
        app.run();
    }
}
