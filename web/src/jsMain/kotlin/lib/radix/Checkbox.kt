@file:JsModule("@radix-ui/react-checkbox") @file:JsNonModule

package lib.radix

import react.ComponentType

/*

Examples

Indeterminate
You can set the checkbox to indeterminate by taking control of its state.

import { DividerHorizontalIcon, CheckIcon } from "@radix-ui/react-icons";
import { Checkbox } from "radix-ui";

export default () => {
	const [checked, setChecked] = React.useState("indeterminate");

	return (
		<>
			<StyledCheckbox checked={checked} onCheckedChange={setChecked}>
				<Checkbox.Indicator>
					{checked === "indeterminate" && <DividerHorizontalIcon />}
					{checked === true && <CheckIcon />}
				</Checkbox.Indicator>
			</StyledCheckbox>

			<button
				type="button"
				onClick={() =>
					setChecked((prevIsChecked) =>
						prevIsChecked === "indeterminate" ? false : "indeterminate",
					)
				}
			>
				Toggle indeterminate
			</button>
		</>
	);
};

*/

// ------------------------------
// Root (Checkbox)
// ------------------------------
external interface CheckboxRootProps : RadixProps, PropsWithAsChild {
   /**
    * The checked state of the checkbox when it is initially rendered.
    * Use when you do not need to control its checked state.
    */
   var defaultChecked: dynamic // boolean | "indeterminate"

   /**
    * The controlled checked state of the checkbox. Must be used in
    * conjunction with onCheckedChange.
    */
   var checked: dynamic // boolean | "indeterminate"

   /**
    * Event handler called when the checked state of the checkbox changes.
    */
   var onCheckedChange: ((dynamic) -> Unit)?

   /**
    * When true, prevents the user from interacting with the checkbox.
    */
   var disabled: Boolean?

   /**
    * When true, indicates that the user must check the checkbox before
    * the owning form can be submitted.
    */
   var required: Boolean?

   /**
    * The name of the checkbox. Submitted with its owning form as part of a name/value pair.
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
 * Contains all the parts of a checkbox. An input will also render when used within a
 * form to ensure events propagate correctly.
 */
@JsName("Root")
external val CheckboxPrimitiveRoot: ComponentType<CheckboxRootProps>

/**
 * @see CheckboxPrimitiveIndicator
 */
external interface CheckboxIndicatorProps : RadixProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries.
    */
   var forceMount: Boolean?
}

/**
 * Renders when the checkbox is in a checked or indeterminate state. You can style
 * this element directly, or you can use it as a wrapper to put an icon into, or both.
 */
@JsName("Indicator")
external val CheckboxPrimitiveIndicator: ComponentType<CheckboxIndicatorProps>
