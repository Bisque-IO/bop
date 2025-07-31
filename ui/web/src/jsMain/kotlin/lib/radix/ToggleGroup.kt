@file:JsModule("@radix-ui/react-toggle-group")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import the component.

import { ToggleGroup } from "radix-ui";

export default () => (
	<ToggleGroup.Root>
		<ToggleGroup.Item />
	</ToggleGroup.Root>
);

Examples
Ensuring there is always a value
You can control the component to ensure a value.

import * as React from "react";
import { ToggleGroup } from "radix-ui";

export default () => {
	const [value, setValue] = React.useState("left");

	return (
		<ToggleGroup.Root
			type="single"
			value={value}
			onValueChange={(value) => {
				if (value) setValue(value);
			}}
		>
			<ToggleGroup.Item value="left">
				<TextAlignLeftIcon />
			</ToggleGroup.Item>
			<ToggleGroup.Item value="center">
				<TextAlignCenterIcon />
			</ToggleGroup.Item>
			<ToggleGroup.Item value="right">
				<TextAlignRightIcon />
			</ToggleGroup.Item>
		</ToggleGroup.Root>
	);
};

Accessibility
Uses roving tabindex to manage focus movement among items.

Keyboard Interactions
Key	      Description
Tab         Moves focus to either the pressed item or the first item in the group.
Space       Activates/deactivates the item.
Enter       Activates/deactivates the item.
ArrowDown   Moves focus to the next item in the group.
ArrowRight  Moves focus to the next item in the group.
ArrowUp     Moves focus to the previous item in the group.
ArrowLeft   Moves focus to the previous item in the group.
Home        Moves focus to the first item.
End         Moves focus to the last item.

*/

// ------------------------------
// ToggleGroup Root
external interface ToggleGroupProps : RadixProps, PropsWithAsChild {
    var type: String // "single" | "multiple"
    var value: Array<String>?
    var defaultValue: Array<String>?
    var onValueChange: ((value: String) -> Unit)?
    var rovingFocus: Boolean?
    var orientation: String? // "horizontal" | "vertical"
    var dir: String? // "ltr" | "rtl"
    var loop: Boolean?
    var disabled: Boolean?
}

@JsName("Root")
external val ToggleGroup: ComponentType<ToggleGroupProps>

// ------------------------------
// ToggleGroup Item
external interface ToggleGroupItemProps : RadixProps {
    var value: String
    var disabled: Boolean?
    var pressed: Boolean?
    var defaultPressed: Boolean?
    var onPressedChange: ((Boolean) -> Unit)?
    var asChild: Boolean?
}

@JsName("Item")
external val ToggleGroupItem: ComponentType<ToggleGroupItemProps>
