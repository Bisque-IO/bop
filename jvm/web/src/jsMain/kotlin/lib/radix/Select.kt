@file:JsModule("@radix-ui/react-select") @file:JsNonModule

package lib.radix

import react.ComponentType
import web.events.Event
import web.html.HTMLElement
import web.uievents.KeyboardEvent

/*

Anatomy
Import all parts and piece them together.

import { Select } from "radix-ui";

export default () => (
	<Select.Root>
		<Select.Trigger>
			<Select.Value />
			<Select.Icon />
		</Select.Trigger>

		<Select.Portal>
			<Select.Content>
				<Select.ScrollUpButton />
				<Select.Viewport>
					<Select.Item>
						<Select.ItemText />
						<Select.ItemIndicator />
					</Select.Item>

					<Select.Group>
						<Select.Label />
						<Select.Item>
							<Select.ItemText />
							<Select.ItemIndicator />
						</Select.Item>
					</Select.Group>

					<Select.Separator />
				</Select.Viewport>
				<Select.ScrollDownButton />
				<Select.Arrow />
			</Select.Content>
		</Select.Portal>
	</Select.Root>
);


Examples
Change the positioning mode
By default, Select will behave similarly to a native macOS menu by positioning SelectContent
relative to the active item. If you preferred an alternative positioning approach similar to
Popover or DropdownMenu then you can set position to popper and make use of additional
alignment options such as side, sideOffset and more.

// index.jsx
import { Select } from "radix-ui";

export default () => (
	<Select.Root>
		<Select.Trigger>…</Select.Trigger>
		<Select.Portal>
			<Select.Content position="popper" sideOffset={5}>
				…
			</Select.Content>
		</Select.Portal>
	</Select.Root>
);

Constrain the content size
When using position="popper" on Select.Content, you may want to constrain the width of the
content so that it matches the trigger width. You may also want to constrain its height to
not exceed the viewport.

We expose several CSS custom properties such as --radix-select-trigger-width and
--radix-select-content-available-height to support this. Use them to constrain
the content dimensions.

// index.jsx
import { Select } from "radix-ui";
import "./styles.css";

export default () => (
	<Select.Root>
		<Select.Trigger>…</Select.Trigger>
		<Select.Portal>
			<Select.Content
				className="SelectContent"
				position="popper"
				sideOffset={5}
			>
				…
			</Select.Content>
		</Select.Portal>
	</Select.Root>
);
/* styles.css */
.SelectContent {
	width: var(--radix-select-trigger-width);
	max-height: var(--radix-select-content-available-height);
}

With disabled items
You can add special styles to disabled items via the data-disabled attribute.

// index.jsx
import { Select } from "radix-ui";
import "./styles.css";

export default () => (
	<Select.Root>
		<Select.Trigger>…</Select.Trigger>
		<Select.Portal>
			<Select.Content>
				<Select.Viewport>
					<Select.Item className="SelectItem" disabled>
						…
					</Select.Item>
					<Select.Item>…</Select.Item>
					<Select.Item>…</Select.Item>
				</Select.Viewport>
			</Select.Content>
		</Select.Portal>
	</Select.Root>
);
/* styles.css */
.SelectItem[data-disabled] {
	color: "gainsboro";
}

With a placeholder
You can use the placeholder prop on Value for when the select has no value.
There's also a data-placeholder attribute on Trigger to help with styling.

// index.jsx
import { Select } from "radix-ui";
import "./styles.css";

export default () => (
	<Select.Root>
		<Select.Trigger className="SelectTrigger">
			<Select.Value placeholder="Pick an option" />
			<Select.Icon />
		</Select.Trigger>
		<Select.Portal>
			<Select.Content>…</Select.Content>
		</Select.Portal>
	</Select.Root>
);
/* styles.css */
.SelectTrigger[data-placeholder] {
	color: "gainsboro";
}
With separators
Use the Separator part to add a separator between items.

<Select.Root>
	<Select.Trigger>…</Select.Trigger>
	<Select.Portal>
		<Select.Content>
			<Select.Viewport>
				<Select.Item>…</Select.Item>
				<Select.Item>…</Select.Item>
				<Select.Item>…</Select.Item>
				<Select.Separator />
				<Select.Item>…</Select.Item>
				<Select.Item>…</Select.Item>
			</Select.Viewport>
		</Select.Content>
	</Select.Portal>
</Select.Root>
With grouped items
Use the Group and Label parts to group items in a section.

<Select.Root>
	<Select.Trigger>…</Select.Trigger>
	<Select.Portal>
		<Select.Content>
			<Select.Viewport>
				<Select.Group>
					<Select.Label>Label</Select.Label>
					<Select.Item>…</Select.Item>
					<Select.Item>…</Select.Item>
					<Select.Item>…</Select.Item>
				</Select.Group>
			</Select.Viewport>
		</Select.Content>
	</Select.Portal>
</Select.Root>
With complex items
You can use custom content in your items.

import { Select } from "radix-ui";

export default () => (
	<Select.Root>
		<Select.Trigger>…</Select.Trigger>
		<Select.Portal>
			<Select.Content>
				<Select.Viewport>
					<Select.Item>
						<Select.ItemText>
							<img src="…" />
							Adolfo Hess
						</Select.ItemText>
						<Select.ItemIndicator>…</Select.ItemIndicator>
					</Select.Item>
					<Select.Item>…</Select.Item>
					<Select.Item>…</Select.Item>
				</Select.Viewport>
			</Select.Content>
		</Select.Portal>
	</Select.Root>
);

Controlling the value displayed in the trigger
By default the trigger will automatically display the selected item ItemText's content.
You can control what appears by chosing to put things inside/outside the ItemText part.

If you need more flexibility, you can control the component using value/onValueChange
props and passing children to SelectValue. Remember to make sure what you put in there
is accessible.

const countries = { france: "🇫🇷", "united-kingdom": "🇬🇧", spain: "🇪🇸" };

export default () => {
	const [value, setValue] = React.useState("france");
	return (
		<Select.Root value={value} onValueChange={setValue}>
			<Select.Trigger>
				<Select.Value aria-label={value}>
					{countries[value]}
				</Select.Value>
				<Select.Icon />
			</Select.Trigger>
			<Select.Portal>
			<Select.Content>
				<Select.Viewport>
					<Select.Item value="france">
						<Select.ItemText>France</Select.ItemText>
						<Select.ItemIndicator>…</Select.ItemIndicator>
					</Select.Item>
					<Select.Item value="united-kingdom">
						<Select.ItemText>United Kingdom</Select.ItemText>
						<Select.ItemIndicator>…</Select.ItemIndicator>
					</Select.Item>
					<Select.Item value="spain">
						<Select.ItemText>Spain</Select.ItemText>
						<Select.ItemIndicator>…</Select.ItemIndicator>
					</Select.Item>
				</Select.Viewport>
			</Select.Content>
			</Select.Portal>
		</Select.Root>
	);
};

With custom scrollbar
The native scrollbar is hidden by default as we recommend using the ScrollUpButton and
ScrollDownButton parts for the best UX. If you do not want to use these parts, compose
your select with our Scroll Area primitive.

// index.jsx
import { Select, ScrollArea } from "radix-ui";
import "./styles.css";

export default () => (
	<Select.Root>
		<Select.Trigger>…</Select.Trigger>
		<Select.Portal>
			<Select.Content>
				<ScrollArea.Root className="ScrollAreaRoot" type="auto">
					<Select.Viewport asChild>
						<ScrollArea.Viewport className="ScrollAreaViewport">
							<StyledItem>…</StyledItem>
							<StyledItem>…</StyledItem>
							<StyledItem>…</StyledItem>
						</ScrollArea.Viewport>
					</Select.Viewport>
					<ScrollArea.Scrollbar
						className="ScrollAreaScrollbar"
						orientation="vertical"
					>
						<ScrollArea.Thumb className="ScrollAreaThumb" />
					</ScrollArea.Scrollbar>
				</ScrollArea.Root>
			</Select.Content>
		</Select.Portal>
	</Select.Root>
);
/* styles.css */
.ScrollAreaRoot {
	width: 100%;
	height: 100%;
}

.ScrollAreaViewport {
	width: 100%;
	height: 100%;
}

.ScrollAreaScrollbar {
	width: 4px;
	padding: 5px 2px;
}

.ScrollAreaThumb {
	background: rgba(0, 0, 0, 0.3);
	border-radius: 3px;
}

Accessibility
Adheres to the ListBox WAI-ARIA design pattern.

See the W3C Select-Only Combobox example for more information.

Keyboard Interactions
Key               Description
Space             When focus is on Select.Trigger, opens the select and focuses the selected item.
                  When focus is on an item, selects the focused item.
Enter             When focus is on Select.Trigger, opens the select and focuses the first item.
                  When focus is on an item, selects the focused item.
ArrowDown         When focus is on Select.Trigger, opens the select.
                  When focus is on an item, moves focus to the next item.
ArrowUp           When focus is on Select.Trigger, opens the select.
                  When focus is on an item, moves focus to the previous item.
Esc               Closes the select and moves focus to Select.Trigger.

Labelling
Use our Label component in order to offer a visual and accessible label for the select.

import { Select, Label } from "radix-ui";

export default () => (
	<>
		<Label>
			Country
			<Select.Root>…</Select.Root>
		</Label>

		{/* or */}

		<Label htmlFor="country">Country</Label>
		<Select.Root>
			<Select.Trigger id="country">…</Select.Trigger>
			<Select.Portal>
				<Select.Content>…</Select.Content>
			</Select.Portal>
		</Select.Root>
	</>
);
Custom APIs
Create your own API by abstracting the primitive parts into your own component.

Abstract down to
Select
and
SelectItem
This example abstracts most of the parts.

Usage
import { Select, SelectItem } from "./your-select";

export default () => (
	<Select defaultValue="2">
		<SelectItem value="1">Item 1</SelectItem>
		<SelectItem value="2">Item 2</SelectItem>
		<SelectItem value="3">Item 3</SelectItem>
	</Select>
);
Implementation
// your-select.jsx
import * as React from "react";
import { Select as SelectPrimitive } from "radix-ui";
import {
	CheckIcon,
	ChevronDownIcon,
	ChevronUpIcon,
} from "@radix-ui/react-icons";

export const Select = React.forwardRef(
	({ children, ...props }, forwardedRef) => {
		return (
			<SelectPrimitive.Root {...props}>
				<SelectPrimitive.Trigger ref={forwardedRef}>
					<SelectPrimitive.Value />
					<SelectPrimitive.Icon>
						<ChevronDownIcon />
					</SelectPrimitive.Icon>
				</SelectPrimitive.Trigger>
				<SelectPrimitive.Portal>
					<SelectPrimitive.Content>
						<SelectPrimitive.ScrollUpButton>
							<ChevronUpIcon />
						</SelectPrimitive.ScrollUpButton>
						<SelectPrimitive.Viewport>{children}</SelectPrimitive.Viewport>
						<SelectPrimitive.ScrollDownButton>
							<ChevronDownIcon />
						</SelectPrimitive.ScrollDownButton>
					</SelectPrimitive.Content>
				</SelectPrimitive.Portal>
			</SelectPrimitive.Root>
		);
	},
);

export const SelectItem = React.forwardRef(
	({ children, ...props }, forwardedRef) => {
		return (
			<SelectPrimitive.Item {...props} ref={forwardedRef}>
				<SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
				<SelectPrimitive.ItemIndicator>
					<CheckIcon />
				</SelectPrimitive.ItemIndicator>
			</SelectPrimitive.Item>
		);
	},
);
*/

/**
 * @see SelectPrimitiveRoot
 */
external interface SelectRootProps : RadixProps {
   /**
    * The value of the select when initially rendered. Use when you do
    * not need to control the state of the select.
    */
   var defaultValue: String?

   /**
    * The controlled value of the select. Should be used in conjunction with onValueChange.
    */
   var value: String?

   /**
    * Event handler called when the value changes.
    */
   var onValueChange: ((value: String) -> Unit)?

   /**
    * The open state of the select when it is initially rendered.
    * Use when you do not need to control its open state.
    */
   var defaultOpen: Boolean?

   /**
    * The controlled open state of the select. Must be used in conjunction with onOpenChange.
    */
   var open: Boolean?

   /**
    * Event handler called when the open state of the select changes.
    */
   var onOpenChange: ((open: Boolean) -> Unit)?

   /**
    * The reading direction of the select when applicable. If omitted, inherits
    * globally from DirectionProvider or assumes LTR (left-to-right) reading mode.
    */
   var dir: String?

   /**
    * The name of the select. Submitted with its owning form as part of a name/value pair.
    */
   var name: String?

   /**
    * When true, prevents the user from interacting with select.
    */
   var disabled: Boolean?

   /**
    * When true, indicates that the user must select a value before the owning form can be submitted.
    */
   var required: Boolean?
}

/**
 * Contains all the parts of a select.
 */
@JsName("Root")
external val SelectPrimitiveRoot: ComponentType<SelectRootProps>

/**
 * @see SelectPrimitiveTrigger
 */
external interface SelectTriggerProps : RadixProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: OpenOrClosed?

   @JsName("data-disabled")
   var dataDisabled: Boolean?

   @JsName("data-placeholder")
   var dataPlaceholder: Any?
}

/**
 * The button that toggles the select. The SelectContent will
 * position itself by aligning over the trigger.
 */
@JsName("Trigger")
external val SelectPrimitiveTrigger: ComponentType<SelectTriggerProps>

/**
 * @see SelectPrimitiveValue
 */
external interface SelectValueProps : RadixProps, PropsWithAsChild {
   /**
    * The content that will be rendered inside the SelectValue when no value or defaultValue is set.
    */
   var placeholder: react.ReactNode?
}

/**
 * The part that reflects the selected value. By default, the selected item's text will
 * be rendered. if you require more control, you can instead control the select and pass
 * your own children. It should not be styled to ensure correct positioning. An optional
 * placeholder prop is also available for when the select has no value.
 */
@JsName("Value")
external val SelectPrimitiveValue: ComponentType<SelectValueProps>

/**
 * @see SelectPrimitiveIcon
 */
external interface SelectIconProps : RadixProps, PropsWithAsChild

/**
 * A small icon often displayed next to the value as a visual affordance for the fact it can
 * be open. By default, renders ▼ but you can use your own icon via asChild or use children.
 */
@JsName("Icon")
external val SelectPrimitiveIcon: ComponentType<SelectIconProps>

/**
 * @see SelectPrimitivePortal
 */
external interface SelectPortalProps : RadixProps {
   /**
    * Specify a container element to portal the content into.
    */
   var container: HTMLElement?
}

/**
 * When used, portals the content part into the body.
 */
@JsName("Portal")
external val SelectPrimitivePortal: ComponentType<SelectPortalProps>

/**
 * @see SelectPrimitiveContent
 */
external interface SelectContentProps : RadixProps, PropsWithAsChild {
   /**
    * Event handler called when focus moves to the trigger after closing.
    * It can be prevented by calling event.preventDefault.
    */
   var onCloseAutoFocus: ((Event) -> Unit)?

   /**
    * Event handler called when the escape key is down.
    * It can be prevented by calling event.preventDefault.
    */
   var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

   /**
    * Event handler called when a pointer event occurs outside the bounds
    * of the component. It can be prevented by calling event.preventDefault.
    */
   var onPointerDownOutside: ((Event) -> Unit)?

   /**
    * The positioning mode to use, item-aligned is the default and behaves
    * similarly to a native macOS menu by positioning content relative to
    * the active item. popper positions content in the same way as our
    * other primitives, for example Popover or DropdownMenu.
    */
   var position: SelectPosition? // "popper" or "item-aligned"

   /**
    * The preferred side of the anchor to render against when open.
    * Will be reversed when collisions occur and avoidCollisions is
    * enabled. Only available when position is set to popper.
    */
   var side: Side?

   /**
    * The distance in pixels from the anchor. Only available when position is set to popper.
    */
   var sideOffset: Int?

   /**
    * The preferred alignment against the anchor. May change when collisions
    * occur. Only available when position is set to popper.
    */
   var align: Align?

   /**
    * An offset in pixels from the "start" or "end" alignment options.
    * Only available when position is set to popper.
    */
   var alignOffset: Int?

   /**
    * When true, overrides the side andalign preferences to prevent collisions
    * with boundary edges. Only available when position is set to popper.
    */
   var avoidCollisions: Boolean?

   /**
    * The element used as the collision boundary. By default, this is the viewport,
    * though you can provide additional element(s) to be included in this check.
    * Only available when position is set to popper.
    */
   var collisionBoundary: CollisionBoundary?

   /**
    * The distance in pixels from the boundary edges where collision detection should occur.
    * Accepts a number (same for all sides), or a partial padding object, for example:
    *
    * { top: 20, left: 20 }
    *
    * Only available when position is set to popper.
    */
   var collisionPadding: CollisionPadding?

   /**
    * The padding between the arrow and the edges of the content. If your content has
    * border-radius, this will prevent it from overflowing the corners. Only available
    * when position is set to popper.
    */
   var arrowPadding: Int?

   /**
    * The sticky behavior on the align axis. "partial" will keep the content in the boundary
    * as long as the trigger is at least partially in the boundary whilst "always" will keep
    * the content in the boundary regardless. Only available when position is set to popper.
    */
   var sticky: Sticky?

   /**
    * Whether to hide the content when the trigger becomes fully occluded. Only available
    * when position is set to popper.
    */
   var hideWhenDetached: Boolean?

   @JsName("data-state")
   var dataState: OpenOrClosed?
   @JsName("data-side")
   var dataSide: Side?
   @JsName("data-align")
   var dataAlign: Align?
}

/*
CSS Variable                              Description
--radix-select-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets. Only present when position="popper".
--radix-select-content-available-width	   The remaining width between the trigger and the boundary edge. Only present when position="popper".
--radix-select-content-available-height	The remaining height between the trigger and the boundary edge. Only present when position="popper".
--radix-select-trigger-width              The width of the trigger. Only present when position="popper".
--radix-select-trigger-height             The height of the trigger. Only present when position="popper".
*/

/**
 * The component that pops out when the select is open.
 */
@JsName("Content")
external val SelectPrimitiveContent: ComponentType<SelectContentProps>

/**
 * @see SelectPrimitiveViewport
 */
external interface SelectViewportProps : RadixProps, PropsWithAsChild

/**
 * The scrolling viewport that contains all the items.
 */
@JsName("Viewport")
external val SelectPrimitiveViewport: ComponentType<SelectViewportProps>

/**
 * @see SelectPrimitiveItem
 */
external interface SelectItemProps : RadixProps, PropsWithAsChild {
   /**
    * The value given as data when submitted with a name.
    */
   var value: String

   /**
    * When true, prevents the user from interacting with the item.
    */
   var disabled: Boolean?

   /**
    * Optional text used for typeahead purposes. By default, the typeahead behavior will
    * use the .textContent of the Select.ItemText part. Use this when the content is
    * complex, or you have non-textual content inside.
    */
   var textValue: String?

   @JsName("data-state")
   var dataState: CheckboxState?
   @JsName("data-highlighted")
   var dataHighlighted: Boolean?
   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/**
 * The component that contains the select items.
 */
@JsName("Item")
external val SelectPrimitiveItem: ComponentType<SelectItemProps>

/**
 * @see SelectPrimitiveItemText
 */
external interface SelectItemTextProps : RadixProps, PropsWithAsChild

/**
 * The textual part of the item. It should only contain the text you want to see
 * in the trigger when that item is selected. It should not be styled to ensure
 * correct positioning.
 */
@JsName("ItemText")
external val SelectPrimitiveItemText: ComponentType<SelectItemTextProps>

/**
 * @see SelectPrimitiveGroup
 */
external interface SelectGroupProps : RadixProps, PropsWithAsChild

/**
 * Used to group multiple items. use in conjunction with SelectLabel to
 * ensure good accessibility via automatic labelling.
 */
@JsName("Group")
external val SelectPrimitiveGroup: ComponentType<SelectGroupProps>

/**
 * @see SelectPrimitiveItemIndicator
 */
external interface SelectItemIndicatorProps : RadixProps, PropsWithAsChild

/**
 * Renders when the item is selected. You can style this element directly,
 * or you can use it as a wrapper to put an icon into, or both.
 */
@JsName("ItemIndicator")
external val SelectPrimitiveItemIndicator: ComponentType<SelectItemIndicatorProps>

/**
 * @see SelectPrimitiveScrollUpButton
 */
external interface SelectScrollUpButtonProps : RadixProps, PropsWithAsChild

/**
 * An optional button used as an affordance to show the viewport overflow as
 * well as functionally enable scrolling upwards.
 */
@JsName("ScrollUpButton")
external val SelectPrimitiveScrollUpButton: ComponentType<SelectScrollUpButtonProps>

/**
 * @see SelectPrimitiveScrollDownButton
 */
external interface SelectScrollDownButtonProps : RadixProps, PropsWithAsChild

/**
 * An optional button used as an affordance to show the viewport overflow as
 * well as functionally enable scrolling downwards.
 */
@JsName("ScrollDownButton")
external val SelectPrimitiveScrollDownButton: ComponentType<SelectScrollDownButtonProps>

/**
 * @see SelectPrimitiveLabel
 */
external interface SelectLabelProps : RadixProps, PropsWithAsChild

/**
 * Used to render the label of a group. It won't be focusable using arrow keys.
 */
@JsName("Label")
external val SelectPrimitiveLabel: ComponentType<SelectLabelProps>

/**
 * @see SelectPrimitiveSeparator
 */
external interface SelectSeparatorProps : RadixProps, PropsWithAsChild

/**
 * Used to visually separate items in the select.
 */
@JsName("Separator")
external val SelectPrimitiveSeparator: ComponentType<SelectSeparatorProps>

/**
 * @see SelectPrimitiveArrow
 */
external interface SelectArrowProps : RadixProps, PropsWithAsChild {
   /**
    * The width of the arrow in pixels.
    */
   var width: Int?

   /**
    * The height of the arrow in pixels.
    */
   var height: Int?
}

/**
 * An optional arrow element to render alongside the content. This can be used to
 * help visually link the trigger with the SelectContent. Must be rendered inside
 * SelectContent. Only available when position is set to popper.
 */
@JsName("Arrow")
external val SelectPrimitiveArrow: ComponentType<SelectArrowProps>
