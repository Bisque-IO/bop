@file:JsModule("@radix-ui/react-toggle")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import the component.

import { Toggle } from "radix-ui";

export default () => <Toggle.Root />;

Accessibility
Keyboard Interactions
Key	  Description
Space   Activates/deactivates the toggle.
Enter   Activates/deactivates the toggle.

*/

/**
 * @see TogglePrimitiveRoot
 */
external interface ToggleProps : RadixProps, PropsWithAsChild {
    /**
     * The pressed state of the toggle when it is initially rendered.
     * Use when you do not need to control its pressed state.
     */
    var defaultPressed: Boolean?

    /**
     * The controlled pressed state of the toggle. Must be used in conjunction with onPressedChange.
     */
    var pressed: Boolean?

    /**
     * Event handler called when the pressed state of the toggle changes.
     */
    var onPressedChange: ((pressed: Boolean) -> Unit)?

    /**
     * When true, prevents the user from interacting with the toggle.
     */
    var disabled: Boolean?

    @JsName("data-state")
    var dataState: String? // "on" | "off"
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * The toggle.
 */
@JsName("Root")
external val TogglePrimitiveRoot: ComponentType<ToggleProps>
