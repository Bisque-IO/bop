@file:JsModule("@radix-ui/react-accordion") @file:JsNonModule

package radix.ui

import react.ComponentType
import react.PropsWithValue

/*

val MyAccordion = FC {
    AccordionRoot {
        type = "single"
        collapsible = true
        defaultValue = "item-1"

        AccordionItem {
            value = "item-1"
            AccordionTrigger {
                +"Item 1"
            }
            AccordionContent {
                div { +"This is the content of item 1." }
            }
        }

        AccordionItem {
            value = "item-2"
            AccordionTrigger {
                +"Item 2"
            }
            AccordionContent {
                div { +"This is the content of item 2." }
            }
        }
    }
}

Keyboard Interactions

Key	      Description
-------------------------------------------------------------------------------------------------
Space       When focus is on an Accordion.Trigger of a collapsed section, expands the section.
Enter       When focus is on an Accordion.Trigger of a collapsed section, expands the section.
Tab         Moves focus to the next focusable element.
Shift+Tab   Moves focus to the previous focusable element.
ArrowDown   Moves focus to the next Accordion.Trigger when orientation is vertical.
ArrowUp     Moves focus to the previous Accordion.Trigger when orientation is vertical.
ArrowRight  Moves focus to the next Accordion.Trigger when orientation is horizontal.
ArrowLeft   Moves focus to the previous Accordion.Trigger when orientation is horizontal.
Home        When focus is on an Accordion.Trigger, moves focus to the first Accordion.Trigger.
End         When focus is on an Accordion.Trigger, moves focus to the last Accordion.Trigger.

*/

/**
 * @see AccordionRoot
 */
external interface AccordionRootProps : DefaultProps, PropsWithAsChild {
   /**
    * Determines whether one or multiple items can be opened at the same time.
    */
   var type: String // "single" | "multiple"

   /**
    * The controlled value of the item to expand when type is "single".
    * Must be used in conjunction with onValueChange.
    */
   var value: dynamic // String or Array<String>

   /**
    * The value of the item to expand when initially rendered and type is "single".
    * Use when you do not need to control the state of the items.
    */
   var defaultValue: dynamic // String or Array<String>

   /**
    * Event handler called when the expanded state of an item changes and type is "multiple".
    */
   var onValueChange: ((value: Array<String>) -> Unit)?

   /**
    * When type is "single", allows closing content when clicking trigger for an open item.
    */
   var collapsible: Boolean?

   /**
    * When true, prevents the user from interacting with the accordion and all its items.
    */
   var disabled: Boolean?

   /**
    * The reading direction of the accordion when applicable.
    * If omitted, assumes LTR (left-to-right) reading mode.
    */
   var dir: String? // "ltr" | "rtl"

   /**
    * The orientation of the accordion.
    */
   var orientation: String? // "vertical" | "horizontal"

   @JsName("data-orientation")
   var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * Contains all the parts of an accordion.
 */
@JsName("Root")
external val AccordionRoot: ComponentType<AccordionRootProps>

/**
 * @see AccordionItem
 */
external interface AccordionItemProps : DefaultProps, PropsWithValue<String>, PropsWithAsChild {
   /**
    * When true, prevents the user from interacting with the item.
    */
   var disabled: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-disabled")
   var dataDisabled: Boolean? // Present when disabled

   @JsName("data-orientation")
   var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * Contains all the parts of a collapsible section.
 */
@JsName("Item")
external val AccordionItem: ComponentType<AccordionItemProps>

/**
 * @see AccordionHeader
 */
external interface AccordionHeaderProps : DefaultProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-disabled")
   var dataDisabled: Boolean? // Present when disabled

   @JsName("data-orientation")
   var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * Wraps an AccordionTrigger. Use the asChild prop to update it to the
 * appropriate heading level for your page.
 */
@JsName("Header")
external val AccordionHeader: ComponentType<AccordionHeaderProps>

/**
 * @see AccordionTrigger
 */
external interface AccordionTriggerProps : DefaultProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-disabled")
   var dataDisabled: Boolean? // Present when disabled

   @JsName("data-orientation")
   var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * Toggles the collapsed state of its associated item. It should be nested inside an AccordionHeader.
 */
@JsName("Trigger")
external val AccordionTrigger: ComponentType<AccordionTriggerProps>

/**
 * @see AccordionContent
 */
external interface AccordionContentProps : DefaultProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries.
    */
   var forceMount: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-disabled")
   var dataDisabled: Boolean? // Present when disabled

   @JsName("data-orientation")
   var dataOrientation: String? // "vertical" | "horizontal"
}

/*
CSS Variable	                  Description
--radix-accordion-content-width	The width of the content when it opens/closes
--radix-accordion-content-height The height of the content when it opens/closes
 */

/**
 * Contains the collapsible content for an item.
 */
@JsName("Content")
external val AccordionContent: ComponentType<AccordionContentProps>
