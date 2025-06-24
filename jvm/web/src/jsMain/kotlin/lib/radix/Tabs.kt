@file:JsModule("@radix-ui/react-tabs")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import all parts and piece them together.

import { Tabs } from "radix-ui";

export default () => (
	<Tabs.Root>
		<Tabs.List>
			<Tabs.Trigger />
		</Tabs.List>
		<Tabs.Content />
	</Tabs.Root>
);

Examples
Vertical
You can create vertical tabs by using the orientation prop.

import { Tabs } from "radix-ui";

export default () => (
	<Tabs.Root defaultValue="tab1" orientation="vertical">
		<Tabs.List aria-label="tabs example">
			<Tabs.Trigger value="tab1">One</Tabs.Trigger>
			<Tabs.Trigger value="tab2">Two</Tabs.Trigger>
			<Tabs.Trigger value="tab3">Three</Tabs.Trigger>
		</Tabs.List>
		<Tabs.Content value="tab1">Tab one content</Tabs.Content>
		<Tabs.Content value="tab2">Tab two content</Tabs.Content>
		<Tabs.Content value="tab3">Tab three content</Tabs.Content>
	</Tabs.Root>
);
Accessibility
Adheres to the Tabs WAI-ARIA design pattern.

Keyboard Interactions
Key	      Description
Tab         When focus moves onto the tabs, focuses the active trigger. When a trigger is focused, moves focus to the active content.
ArrowDown   Moves focus to the next trigger depending on orientation and activates its associated content.
ArrowRight  Moves focus to the next trigger depending on orientation and activates its associated content.
ArrowUp     Moves focus to the previous trigger depending on orientation and activates its associated content.
ArrowLeft   Moves focus to the previous trigger depending on orientation and activates its associated content.
Home        Moves focus to the first trigger and activates its associated content.
End         Moves focus to the last trigger and activates its associated content.

*/

/**
 * @see TabsPrimitiveRoot
 */
external interface TabsRootProps : RadixProps, PropsWithAsChild {
    /**
     * The value of the tab that should be active when initially rendered.
     * Use when you do not need to control the state of the tabs.
     */
    var defaultValue: String?

    /**
     * The controlled value of the tab to activate. Should be used in conjunction with onValueChange.
     */
    var value: String?

    /**
     * Event handler called when the value changes.
     */
    var onValueChange: ((value: String) -> Unit)?

    /**
     * The orientation of the component.
     */
    var orientation: HorizontalOrVertical? // "horizontal" | "vertical"

    /**
     * The reading direction of the tabs. If omitted, inherits globally from
     * DirectionProvider or assumes LTR (left-to-right) reading mode.
     */
    var dir: TextDirection? // "ltr" | "rtl"

    /**
     * When automatic, tabs are activated when receiving focus. When manual, tabs are activated when clicked.
     */
    var activationMode: String? // "automatic" | "manual"

    @JsName("data-orientation")
    var dataOrientation: HorizontalOrVertical?
}

/**
 * Contains all the tabs component parts.
 */
@JsName("Root")
external val TabsPrimitiveRoot: ComponentType<TabsRootProps>

/**
 * @see TabsPrimitiveList
 */
external interface TabsListProps : RadixProps, PropsWithAsChild {
    /**
     * When true, keyboard navigation will loop from last tab to first, and vice versa.
     */
    var loop: Boolean?

    @JsName("data-orientation")
    var dataOrientation: HorizontalOrVertical?
}

/**
 * Contains the triggers that are aligned along the edge of the active content.
 */
@JsName("List")
external val TabsPrimitiveList: ComponentType<TabsListProps>

/**
 * @see TabsPrimitiveTrigger
 */
external interface TabsTriggerProps : RadixProps, PropsWithAsChild {
    /**
     * A unique value that associates the trigger with a content.
     */
    var value: String?

    /**
     * When true, prevents the user from interacting with the tab.
     */
    var disabled: Boolean?

    @JsName("data-state")
    var dataState: String? // "active" | "inactive"
    @JsName("data-disabled")
    var dataDisabled: Boolean?
    @JsName("data-orientation")
    var dataOrientation: HorizontalOrVertical?
}

/**
 * The button that activates its associated content.
 */
@JsName("Trigger")
external val TabsPrimitiveTrigger: ComponentType<TabsTriggerProps>

/**
 * @see TabsPrimitiveContent
 */
external interface TabsContentProps : RadixProps, PropsWithAsChild {
    /**
     * A unique value that associates the content with a trigger.
     */
    var value: String?

    /**
     * Used to force mounting when more control is needed. Useful when
     * controlling animation with React animation libraries.
     */
    var forceMount: Boolean?

    @JsName("data-state")
    var dataState: String? // "active" | "inactive"
    @JsName("data-orientation")
    var dataOrientation: HorizontalOrVertical?
}

/**
 * Contains the content associated with each trigger.
 */
@JsName("Content")
external val TabsPrimitiveContent: ComponentType<TabsContentProps>
