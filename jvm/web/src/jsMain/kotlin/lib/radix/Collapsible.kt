@file:JsModule("@radix-ui/react-collapsible") @file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import the .components and piece the parts together.

import { Collapsible } from "radix-ui";

export default () => (
	<Collapsible.Root>
		<Collapsible.Trigger />
		<Collapsible.Content />
	</Collapsible.Root>
);

Examples

Animating content size

Use the --radix-collapsible-content-width and/or --radix-collapsible-content-height CSS variables
to animate the size of the content when it opens/closes. Here's a demo:

// index.jsx
import { Collapsible } from "radix-ui";
import "./styles.css";

export default () => (
	<Collapsible.Root>
		<Collapsible.Trigger>…</Collapsible.Trigger>
		<Collapsible.Content className="CollapsibleContent">
			…
		</Collapsible.Content>
	</Collapsible.Root>
);

// styles.css
.CollapsibleContent {
	overflow: hidden;
}
.CollapsibleContent[data-state="open"] {
	animation: slideDown 300ms ease-out;
}
.CollapsibleContent[data-state="closed"] {
	animation: slideUp 300ms ease-out;
}

@keyframes slideDown {
	from {
		height: 0;
	}
	to {
		height: var(--radix-collapsible-content-height);
	}
}

@keyframes slideUp {
	from {
		height: var(--radix-collapsible-content-height);
	}
	to {
		height: 0;
	}
}

*/

/**
 * @see CollapsiblePrimitiveRoot
 */
external interface CollapsibleRootProps : RadixProps, PropsWithAsChild {
   /**
    * The open state of the collapsible when it is initially rendered.
    * Use when you do not need to control its open state.
    */
   var defaultOpen: Boolean?

   /**
    * The controlled open state of the collapsible. Must be used in conjunction with onOpenChange.
    */
   var open: Boolean?

   /**
    * Event handler called when the open state of the collapsible changes.
    */
   var onOpenChange: ((open: Boolean) -> Unit)?

   /**
    * When true, prevents the user from interacting with the collapsible.
    */
   var disabled: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/**
 * Contains all the parts of a collapsible.
 */
@JsName("Root")
external val CollapsiblePrimitiveRoot: ComponentType<CollapsibleRootProps>

/**
 * @see CollapsiblePrimitiveTrigger
 */
external interface CollapsibleTriggerProps : RadixProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/**
 * The button that toggles the collapsible.
 */
@JsName("Trigger")
external val CollapsiblePrimitiveTrigger: ComponentType<CollapsibleTriggerProps>

/**
 * @see CollapsiblePrimitiveContent
 */
external interface CollapsibleContentProps : RadixProps, PropsWithAsChild {
   /**
    * Used to force mounting when more control is needed. Useful when
    * controlling animation with React animation libraries.
    */
   var forceMount: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-disabled")
   var dataDisabled: Boolean?
}

/*
--radix-collapsible-content-width	The width of the content when it opens/closes
--radix-collapsible-content-height  The height of the content when it opens/closes
 */

/**
 * The component that contains the collapsible content.
 */
@JsName("Content")
external val CollapsiblePrimitiveContent: ComponentType<CollapsibleContentProps>
