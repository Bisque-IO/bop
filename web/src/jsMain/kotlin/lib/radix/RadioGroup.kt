@file:JsModule("@radix-ui/react-radio-group")
@file:JsNonModule

package lib.radix

import react.ComponentType
import react.dom.html.HTMLAttributes

/*

Anatomy
Import all parts and piece them together.

import { RadioGroup } from "radix-ui";

export default () => (
	<RadioGroup.Root>
		<RadioGroup.Item>
			<RadioGroup.Indicator />
		</RadioGroup.Item>
	</RadioGroup.Root>
);

Accessibility
Adheres to the Radio Group WAI-ARIA design pattern and uses roving tabindex to manage focus movement among radio items.

Keyboard Interactions
Key	          Description
Tab             Moves focus to either the checked radio item or the first radio item in the group.
Space           When focus is on an unchecked radio item, checks it.
ArrowDown       Moves focus and checks the next radio item in the group.
ArrowRight      Moves focus and checks the next radio item in the group.
ArrowUp         Moves focus to the previous radio item in the group.
ArrowLeft       Moves focus to the previous radio item in the group.

*/

/**
 * @see RadioGroupPrimitiveRoot
 */
external interface RadioGroupProps : RadixProps, PropsWithAsChild {
    /**
     * The value of the radio item that should be checked when initially rendered.
     * Use when you do not need to control the state of the radio items.
     */
    var defaultValue: String?

    /**
     * The controlled value of the radio item to check. Should be used in conjunction with onValueChange.
     */
    var value: String?

    /**
     * Event handler called when the value changes.
     */
    var onValueChange: ((value: String) -> Unit)?

    /**
     * When true, prevents the user from interacting with radio items.
     */
    var disabled: Boolean?

    /**
     * The name of the group. Submitted with its owning form as part of a name/value pair.
     */
    var name: String?

    /**
     * When true, indicates that the user must check a radio item before the owning form can be submitted.
     */
    var required: Boolean?

    /**
     * The orientation of the component.
     */
    var orientation: String? // "horizontal" | "vertical"

    /**
     * The reading direction of the radio group. If omitted, inherits globally from
     * DirectionProvider or assumes LTR (left-to-right) reading mode.
     */
    var dir: String? // "ltr" | "rtl"

    /**
     * When true, keyboard navigation will loop from last item to first, and vice versa.
     */
    var loop: Boolean?

    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * Contains all the parts of a radio group.
 */
@JsName("Root")
external val RadioGroupPrimitiveRoot: ComponentType<RadioGroupProps>

/**
 * @see RadioGroupPrimitiveItem
 */
external interface RadioGroupItemProps : RadixProps, PropsWithAsChild {
    /**
     * The value given as data when submitted with a name.
     */
    var value: String?

    /**
     * When true, prevents the user from interacting with the radio item.
     */
    var disabled: Boolean?

    /**
     * When true, indicates that the user must check the radio item before the
     * owning form can be submitted.
     */
    var required: Boolean?

    var id: String?

    @JsName("data-state")
    var dataState: String? // "checked" | "unchecked"

    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * An item in the group that can be checked. An input will also render when
 * used within a form to ensure events propagate correctly.
 */
@JsName("Item")
external val RadioGroupPrimitiveItem: ComponentType<RadioGroupItemProps>

/**
 * @see RadioGroupPrimitiveIndicator
 */
external interface RadioGroupIndicatorProps : RadixProps, PropsWithAsChild {
    /**
     * Used to force mounting when more control is needed. Useful when
     * controlling animation with React animation libraries.
     */
    var forceMount: Boolean?

    @JsName("data-state")
    var dataState: String? // "checked" | "unchecked"

    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * Renders when the radio item is in a checked state. You can style this element
 * directly, or you can use it as a wrapper to put an icon into, or both.
 */
@JsName("Indicator")
external val RadioGroupPrimitiveIndicator: ComponentType<RadioGroupIndicatorProps>
