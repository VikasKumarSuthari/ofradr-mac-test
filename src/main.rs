#![allow(non_snake_case)]
#[macro_use]
extern crate objc;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyAccessory,
    NSWindow, NSWindowStyleMask, NSWindowCollectionBehavior,
    NSBackingStoreType,
};
use objc::runtime::{Class, Object, Sel};
use objc::declare::ClassDecl;
use cocoa::base::{nil, id, YES, NO};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};
use std::sync::Once;
use std::sync::atomic::{AtomicPtr, AtomicBool, Ordering};

// Store the text field globally so event handler can access it
static TEXT_FIELD: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());
// Track if our text field is "active" (receiving input)
static TEXT_FIELD_ACTIVE: AtomicBool = AtomicBool::new(true);

#[allow(non_upper_case_globals)]
const kCGFloatingWindowLevel: i64 = 2147483631;

// NSWindowSharingType values
#[allow(non_upper_case_globals)]
const NSWindowSharingNone: u64 = 0;

// NSBezelStyle values
#[allow(non_upper_case_globals)]
const NSBezelStyleRounded: u64 = 1;

// NSEventType values
#[allow(non_upper_case_globals)]
const NSEventTypeKeyDown: u64 = 10;

// NSEventModifierFlags
#[allow(non_upper_case_globals)]
const NSEventModifierFlagCommand: u64 = 1 << 20;
#[allow(non_upper_case_globals)]
const NSEventModifierFlagOption: u64 = 1 << 19;
#[allow(non_upper_case_globals)]
const NSEventModifierFlagControl: u64 = 1 << 18;

// Special key codes to let pass through
#[allow(non_upper_case_globals)]
const kVK_UpArrow: u16 = 0x7E;
#[allow(non_upper_case_globals)]
const kVK_DownArrow: u16 = 0x7D;
#[allow(non_upper_case_globals)]
const kVK_LeftArrow: u16 = 0x7B;
#[allow(non_upper_case_globals)]
const kVK_RightArrow: u16 = 0x7C;
#[allow(non_upper_case_globals)]
const kVK_Escape: u16 = 0x35;
#[allow(non_upper_case_globals)]
const kVK_Tab: u16 = 0x30;
#[allow(non_upper_case_globals)]
const kVK_Return: u16 = 0x24;
#[allow(non_upper_case_globals)]
const kVK_Delete: u16 = 0x33;

static REGISTER_BUTTON_HANDLER: Once = Once::new();
static REGISTER_DRAGGABLE_VIEW: Once = Once::new();
static REGISTER_FOCUSLESS_TEXT_FIELD: Once = Once::new();

// Button action handler for close button
extern "C" fn close_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    unsafe {
        let app = NSApp();
        let _: () = msg_send![app, terminate: nil];
    }
}

// Button action handler for center test button
extern "C" fn test_button_clicked(_this: &Object, _cmd: Sel, _sender: id) {
    println!("Test button clicked!");
}

// DraggableView mouse event handlers
extern "C" fn mouse_down(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        let window: id = msg_send![this, window];
        let _: () = msg_send![window, performWindowDragWithEvent: event];
    }
}

extern "C" fn accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> bool {
    true // Accept first mouse click without activating
}

// FocuslessTextField - accepts clicks without activating window
extern "C" fn text_field_accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> bool {
    true
}

extern "C" fn text_field_mouse_down(_this: &Object, _cmd: Sel, _event: id) {
    // Activate our text input without stealing focus
    TEXT_FIELD_ACTIVE.store(true, Ordering::SeqCst);
    println!("Text field activated - typing will appear here");
}

extern "C" fn text_field_accepts_first_responder(_this: &Object, _cmd: Sel) -> bool {
    false // Don't become first responder (that would steal focus)
}

extern "C" fn text_field_becomes_first_responder(_this: &Object, _cmd: Sel) -> bool {
    false // Refuse to become first responder
}

fn register_button_handler_class() {
    REGISTER_BUTTON_HANDLER.call_once(|| {
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("ButtonHandler", superclass).unwrap();
        
        unsafe {
            decl.add_method(
                sel!(closeButtonClicked:),
                close_button_clicked as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(testButtonClicked:),
                test_button_clicked as extern "C" fn(&Object, Sel, id),
            );
        }
        
        decl.register();
    });
}

fn register_draggable_view_class() {
    REGISTER_DRAGGABLE_VIEW.call_once(|| {
        let superclass = Class::get("NSView").unwrap();
        let mut decl = ClassDecl::new("DraggableView", superclass).unwrap();
        
        unsafe {
            decl.add_method(
                sel!(mouseDown:),
                mouse_down as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(acceptsFirstMouse:),
                accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool,
            );
        }
        
        decl.register();
    });
}

fn register_focusless_text_field_class() {
    REGISTER_FOCUSLESS_TEXT_FIELD.call_once(|| {
        let superclass = Class::get("NSTextField").unwrap();
        let mut decl = ClassDecl::new("FocuslessTextField", superclass).unwrap();
        
        unsafe {
            decl.add_method(
                sel!(acceptsFirstMouse:),
                text_field_accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool,
            );
            decl.add_method(
                sel!(mouseDown:),
                text_field_mouse_down as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(acceptsFirstResponder),
                text_field_accepts_first_responder as extern "C" fn(&Object, Sel) -> bool,
            );
            decl.add_method(
                sel!(becomeFirstResponder),
                text_field_becomes_first_responder as extern "C" fn(&Object, Sel) -> bool,
            );
        }
        
        decl.register();
    });
}

fn should_pass_through_key(key_code: u16, modifier_flags: u64) -> bool {
    // Let arrow keys, escape, tab pass through
    if key_code == kVK_UpArrow || key_code == kVK_DownArrow || 
       key_code == kVK_LeftArrow || key_code == kVK_RightArrow ||
       key_code == kVK_Escape || key_code == kVK_Tab {
        return true;
    }
    
    // Let keys with Command, Option, or Control pass through (shortcuts)
    if (modifier_flags & NSEventModifierFlagCommand) != 0 ||
       (modifier_flags & NSEventModifierFlagOption) != 0 ||
       (modifier_flags & NSEventModifierFlagControl) != 0 {
        return true;
    }
    
    false
}

fn main() {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        // Register our custom classes
        register_button_handler_class();
        register_draggable_view_class();
        register_focusless_text_field_class();

        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

        let frame = NSRect::new(
            NSPoint::new(200.0, 200.0),
            NSSize::new(400.0, 300.0),
        );

        // Create an NSPanel instead of NSWindow for non-activating behavior
        let panel_class = Class::get("NSPanel").unwrap();
        let window: id = msg_send![panel_class, alloc];
        let window: id = msg_send![window, 
            initWithContentRect:frame 
            styleMask:NSWindowStyleMask::NSBorderlessWindowMask 
            backing:NSBackingStoreType::NSBackingStoreBuffered 
            defer:NO
        ];

        let ns_color_class = Class::get("NSColor").unwrap();
        let black_color: id = msg_send![ns_color_class, blackColor];
        let _: () = msg_send![window, setBackgroundColor: black_color];
        let _: () = msg_send![window, setOpaque: YES];
        let _: () = msg_send![window, setHasShadow: NO];
        let _: () = msg_send![window, setLevel: kCGFloatingWindowLevel];
        
        // Make window invisible in screen recording and screen sharing
        let _: () = msg_send![window, setSharingType: NSWindowSharingNone];
        
        // Make window non-activating - clicks won't steal focus from other apps
        let _: () = msg_send![window, setFloatingPanel: YES];
        let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: YES];
        
        // Set collection behavior to appear on all spaces
        let behavior = NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;
        let _: () = msg_send![window, setCollectionBehavior: behavior];
        
        // Enable window to be movable by dragging background
        let _: () = msg_send![window, setMovableByWindowBackground: YES];

        // Create a draggable view as the content view
        let draggable_class = Class::get("DraggableView").unwrap();
        let content_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0));
        let draggable_view: id = msg_send![draggable_class, alloc];
        let draggable_view: id = msg_send![draggable_view, initWithFrame: content_frame];
        let _: () = msg_send![window, setContentView: draggable_view];

        // Create button handler instance
        let handler_class = Class::get("ButtonHandler").unwrap();
        let handler: id = msg_send![handler_class, new];

        // Create Close button (top-right corner)
        let button_class = Class::get("NSButton").unwrap();
        let close_button: id = msg_send![button_class, alloc];
        let close_button_frame = NSRect::new(NSPoint::new(340.0, 260.0), NSSize::new(50.0, 30.0));
        let close_button: id = msg_send![close_button, initWithFrame: close_button_frame];
        let close_title = NSString::alloc(nil).init_str("Close");
        let _: () = msg_send![close_button, setTitle: close_title];
        let _: () = msg_send![close_button, setBezelStyle: NSBezelStyleRounded];
        let _: () = msg_send![close_button, setTarget: handler];
        let _: () = msg_send![close_button, setAction: sel!(closeButtonClicked:)];
        let _: () = msg_send![draggable_view, addSubview: close_button];

        // Create Test button (right of center)
        let test_button: id = msg_send![button_class, alloc];
        let test_button_frame = NSRect::new(NSPoint::new(280.0, 135.0), NSSize::new(100.0, 30.0));
        let test_button: id = msg_send![test_button, initWithFrame: test_button_frame];
        let test_title = NSString::alloc(nil).init_str("Test");
        let _: () = msg_send![test_button, setTitle: test_title];
        let _: () = msg_send![test_button, setBezelStyle: NSBezelStyleRounded];
        let _: () = msg_send![test_button, setTarget: handler];
        let _: () = msg_send![test_button, setAction: sel!(testButtonClicked:)];
        let _: () = msg_send![draggable_view, addSubview: test_button];

        // Create Focusless Text Field for input
        let text_field_class = Class::get("FocuslessTextField").unwrap();
        let text_field: id = msg_send![text_field_class, alloc];
        let text_field_frame = NSRect::new(NSPoint::new(20.0, 135.0), NSSize::new(250.0, 30.0));
        let text_field: id = msg_send![text_field, initWithFrame: text_field_frame];
        
        // Style the text field
        let placeholder = NSString::alloc(nil).init_str("Type here...");
        let _: () = msg_send![text_field, setPlaceholderString: placeholder];
        let _: () = msg_send![text_field, setBezeled: YES];
        let _: () = msg_send![text_field, setDrawsBackground: YES];
        let white_color: id = msg_send![ns_color_class, whiteColor];
        let _: () = msg_send![text_field, setBackgroundColor: white_color];
        // Make it non-editable - we handle input via global event monitor
        let _: () = msg_send![text_field, setEditable: NO];
        let _: () = msg_send![text_field, setSelectable: NO];
        let _: () = msg_send![draggable_view, addSubview: text_field];
        
        // Store text field reference for event handler
        TEXT_FIELD.store(text_field as *mut Object, Ordering::SeqCst);

        // Set up global event monitor for key events
        let ns_event_class = Class::get("NSEvent").unwrap();
        let mask: u64 = 1 << NSEventTypeKeyDown; // NSEventMaskKeyDown
        
        // Create the event handler block
        let block = block::ConcreteBlock::new(move |event: id| -> id {
            // Only capture keys if our text field is active
            if !TEXT_FIELD_ACTIVE.load(Ordering::SeqCst) {
                return event; // Pass through
            }
            
            let key_code: u16 = msg_send![event, keyCode];
            let modifier_flags: u64 = msg_send![event, modifierFlags];
            
            // Check if we should pass this key through
            if should_pass_through_key(key_code, modifier_flags) {
                return event; // Let it pass through
            }
            
            // Get the text field and append the character
            let tf = TEXT_FIELD.load(Ordering::SeqCst);
            if !tf.is_null() {
                let characters: id = msg_send![event, characters];
                if characters != nil {
                    let current_text: id = msg_send![tf, stringValue];
                    let mutable_string: id = msg_send![class!(NSMutableString), alloc];
                    let mutable_string: id = msg_send![mutable_string, initWithString: current_text];
                    
                    // Handle backspace
                    if key_code == kVK_Delete {
                        let length: usize = msg_send![mutable_string, length];
                        if length > 0 {
                            let range = cocoa::foundation::NSRange::new((length - 1) as u64, 1);
                            let _: () = msg_send![mutable_string, deleteCharactersInRange: range];
                        }
                    } else if key_code == kVK_Return {
                        // Clear on Enter
                        let empty = NSString::alloc(nil).init_str("");
                        let _: () = msg_send![tf, setStringValue: empty];
                        return nil; // Swallow the event
                    } else {
                        let _: () = msg_send![mutable_string, appendString: characters];
                    }
                    
                    let _: () = msg_send![tf, setStringValue: mutable_string];
                }
            }
            
            nil // Return nil to swallow the event (prevents it from reaching other apps)
        });
        let block = block.copy();
        
        let _monitor: id = msg_send![ns_event_class, addGlobalMonitorForEventsMatchingMask:mask handler:&*block];

        let _: () = msg_send![window, center];
        let _: () = msg_send![window, orderFrontRegardless];

        app.run();
    }
}
