@file:JsModule("@radix-ui/react-switch")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Imports all parts and pieces them together.

import { Switch } from "radix-ui";

export default () => (
	<Switch.Root>
		<Switch.Thumb />
	</Switch.Root>
);

Accessibility
Adheres to the
switch
role requirements.

Keyboard Interactions
Key	      Description
Space       Toggles the component's state.
Enter       Toggles the component's state.

*/

/**
 * @see SwitchPrimitiveRoot
 */
external interface SwitchProps : RadixProps, PropsWithAsChild {
    /**
     * The state of the switch when it is initially rendered. Use when you do not need to control its state.
     */
    var defaultChecked: Boolean?

    /**
     * The controlled state of the switch. Must be used in conjunction with onCheckedChange.
     */
    var checked: Boolean?

    /**
     * Event handler called when the state of the switch changes.
     */
    var onCheckedChange: ((checked: Boolean) -> Unit)?

    /**
     * When true, prevents the user from interacting with the switch.
     */
    var disabled: Boolean?

    /**
     * When true, indicates that the user must check the switch before the owning form can be submitted.
     */
    var required: Boolean?

    /**
     * The name of the switch. Submitted with its owning form as part of a name/value pair.
     */
    var name: String?

    /**
     * The value given as data when submitted with a name.
     */
    var value: String?

    @JsName("data-state")
    var dataState: String?
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * Contains all the parts of a switch. An input will also render when used
 * within a form to ensure events propagate correctly.
 */
@JsName("Root")
external val SwitchPrimitiveRoot: ComponentType<SwitchProps>

/**
 * @see SwitchPrimitiveThumb
 */
external interface SwitchThumbProps : RadixProps, PropsWithAsChild {
    @JsName("data-state")
    var dataState: String?
    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * The thumb that is used to visually indicate whether the switch is on or off.
 */
@JsName("Thumb")
external val SwitchPrimitiveThumb: ComponentType<SwitchThumbProps>
