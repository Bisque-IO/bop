@file:JsModule("@radix-ui/react-popover") @file:JsNonModule

package lib.radix

import react.ComponentType
import web.events.Event
import web.html.HTMLElement
import web.uievents.KeyboardEvent

/*

Anatomy
Import all parts and piece them together.

import { Popover } from "radix-ui";

export default () => (
	<Popover.Root>
		<Popover.Trigger />
		<Popover.Anchor />
		<Popover.Portal>
			<Popover.Content>
				<Popover.Close />
				<Popover.Arrow />
			</Popover.Content>
		</Popover.Portal>
	</Popover.Root>
);


Examples
Constrain the content size
You may want to constrain the width of the content so that it matches the trigger width.
You may also want to constrain its height to not exceed the viewport.

We expose several CSS custom properties such as --radix-popover-trigger-width and
--radix-popover-content-available-height to support this. Use them to constrain the content dimensions.

// index.jsx
import { Popover } from "radix-ui";
import "./styles.css";

export default () => (
	<Popover.Root>
		<Popover.Trigger>…</Popover.Trigger>
		<Popover.Portal>
			<Popover.Content className="PopoverContent" sideOffset={5}>
				…
			</Popover.Content>
		</Popover.Portal>
	</Popover.Root>
);
/* styles.css */
.PopoverContent {
	width: var(--radix-popover-trigger-width);
	max-height: var(--radix-popover-content-available-height);
}

Origin-aware animations
We expose a CSS custom property --radix-popover-content-transform-origin. Use it to animate
the content from its computed origin based on side, sideOffset, align, alignOffset and any
collisions.

// index.jsx
import { Popover } from "radix-ui";
import "./styles.css";

export default () => (
	<Popover.Root>
		<Popover.Trigger>…</Popover.Trigger>
		<Popover.Portal>
			<Popover.Content className="PopoverContent">…</Popover.Content>
		</Popover.Portal>
	</Popover.Root>
);
/* styles.css */
.PopoverContent {
	transform-origin: var(--radix-popover-content-transform-origin);
	animation: scaleIn 0.5s ease-out;
}

@keyframes scaleIn {
	from {
		opacity: 0;
		transform: scale(0);
	}
	to {
		opacity: 1;
		transform: scale(1);
	}
}

Collision-aware animations
We expose data-side and data-align attributes. Their values will change at runtime to
reflect collisions. Use them to create collision and direction-aware animations.

// index.jsx
import { Popover } from "radix-ui";
import "./styles.css";

export default () => (
	<Popover.Root>
		<Popover.Trigger>…</Popover.Trigger>
		<Popover.Portal>
			<Popover.Content className="PopoverContent">…</Popover.Content>
		</Popover.Portal>
	</Popover.Root>
);
/* styles.css */
.PopoverContent {
	animation-duration: 0.6s;
	animation-timing-function: cubic-bezier(0.16, 1, 0.3, 1);
}
.PopoverContent[data-side="top"] {
	animation-name: slideUp;
}
.PopoverContent[data-side="bottom"] {
	animation-name: slideDown;
}

@keyframes slideDown {
	from {
		opacity: 0;
		transform: translateY(-10px);
	}
	to {
		opacity: 1;
		transform: translateY(0);
	}
}

@keyframes slideUp {
	from {
		opacity: 0;
		transform: translateY(10px);
	}
	to {
		opacity: 1;
		transform: translateY(0);
	}
}
With custom anchor
You can anchor the content to another element if you do not want to use the trigger as the anchor.

// index.jsx
import { Popover } from "radix-ui";
import "./styles.css";

export default () => (
	<Popover.Root>
		<Popover.Anchor asChild>
			<div className="Row">
				Row as anchor <Popover.Trigger>Trigger</Popover.Trigger>
			</div>
		</Popover.Anchor>

		<Popover.Portal>
			<Popover.Content>…</Popover.Content>
		</Popover.Portal>
	</Popover.Root>
);
/* styles.css */
.Row {
	background-color: gainsboro;
	padding: 20px;
}
Accessibility
Adheres to the Dialog WAI-ARIA design pattern.

Keyboard Interactions
Key	                  Description
Space                   Opens/closes the popover.
Enter                   Opens/closes the popover.
Tab                     Moves focus to the next focusable element
Shift+Tab               Moves focus to the previous focusable element
Esc                     Closes the popover and moves focus to Popover.Trigger.


Custom APIs
Create your own API by abstracting the primitive parts into your own component.

Abstract the arrow and set default configuration
This example abstracts the Popover.Arrow part and sets a default sideOffset configuration.

Usage
import { Popover, PopoverTrigger, PopoverContent } from "./your-popover";

export default () => (
	<Popover>
		<PopoverTrigger>Popover trigger</PopoverTrigger>
		<PopoverContent>Popover content</PopoverContent>
	</Popover>
);
Implementation
// your-popover.jsx
import * as React from "react";
import { Popover as PopoverPrimitive } from "radix-ui";

export const Popover = PopoverPrimitive.Root;
export const PopoverTrigger = PopoverPrimitive.Trigger;

export const PopoverContent = React.forwardRef(
	({ children, ...props }, forwardedRef) => (
		<PopoverPrimitive.Portal>
			<PopoverPrimitive.Content sideOffset={5} {...props} ref={forwardedRef}>
				{children}
				<PopoverPrimitive.Arrow />
			</PopoverPrimitive.Content>
		</PopoverPrimitive.Portal>
	),
);
*/

/**
 * @see PopoverPrimitiveRoot
 */
external interface PopoverRootProps : RadixProps {
   /**
    * The open state of the popover when it is initially rendered.
    * Use when you do not need to control its open state.
    */
   var defaultOpen: Boolean?

   /**
    * The controlled open state of the popover. Must be used in conjunction with onOpenChange.
    */
   var open: Boolean?

   /**
    * Event handler called when the open state of the popover changes.
    */
   var onOpenChange: ((open: Boolean) -> Unit)?

   /**
    * The modality of the popover. When set to true, interaction with outside
    * elements will be disabled and only popover content will be visible to
    * screen readers.
    */
   var modal: Boolean?
}

/**
 * Contains all the parts of a popover.
 */
@JsName("Root")
external val PopoverPrimitiveRoot: ComponentType<PopoverRootProps>

/**
 * @see PopoverPrimitiveTrigger
 */
external interface PopoverTriggerProps : RadixProps, PropsWithAsChild {
   @JsName("data-state")
   var dataState: String? // "open" | "closed"
}

/**
 * The button that toggles the popover. By default, the PopoverContent
 * will position itself against the trigger.
 */
@JsName("Trigger")
external val PopoverPrimitiveTrigger: ComponentType<PopoverTriggerProps>

/**
 * @see PopoverPrimitiveAnchor
 */
external interface PopoverAnchorProps : RadixProps, PropsWithAsChild

/**
 * An optional element to position the PopoverPrimitiveContent against. If this part is
 * not used, the content will position alongside the PopoverPrimitiveTrigger.
 */
@JsName("Anchor")
external val PopoverPrimitiveAnchor: ComponentType<PopoverAnchorProps>

/**
 * @see PopoverPrimitivePortal
 */
external interface PopoverPortalProps : RadixProps {
   /**
    * Used to force mounting when more control is needed. Useful when controlling
    * animation with React animation libraries. If used on this part, it will be
    * inherited by Popover.Content.
    */
   var forceMount: Boolean?

   /**
    * Specify a container element to portal the content into.
    */
   var container: HTMLElement?
}

/**
 * An optional element to position the PopoverPrimitiveContent against. If this part is
 * not used, the content will position alongside the PopoverPrimitiveTrigger.
 */
@JsName("Portal")
external val PopoverPrimitivePortal: ComponentType<PopoverPortalProps>

/**
 * @see PopoverPrimitiveContent
 */
external interface PopoverContentProps : RadixProps, PropsWithAsChild {
   /**
    * Event handler called when focus moves into the component after opening.
    * It can be prevented by calling event.preventDefault.
    */
   var onOpenAutoFocus: ((Event) -> Unit)?

   /**
    * Event handler called when focus moves to the trigger after closing.
    * It can be prevented by calling event.preventDefault.
    */
   var onCloseAutoFocus: ((Event) -> Unit)?

   /**
    * Event handler called when the escape key is down. It can be prevented by calling event.preventDefault.
    */
   var onEscapeKeyDown: ((KeyboardEvent) -> Unit)?

   /**
    * Event handler called when a pointer event occurs outside the bounds of the component.
    * It can be prevented by calling event.preventDefault.
    */
   var onPointerDownOutside: ((Event) -> Unit)?

   /**
    * Event handler called when focus moves outside the bounds of the component.
    * It can be prevented by calling event.preventDefault.
    */
   var onFocusOutside: ((Event) -> Unit)?

   /**
    * Event handler called when an interaction (pointer or focus event) happens outside
    * the bounds of the component. It can be prevented by calling event.preventDefault.
    */
   var onInteractOutside: ((Event) -> Unit)?

   /**
    * Used to force mounting when more control is needed. Useful when controlling animation
    * with React animation libraries. It inherits from PopoverPortal.
    */
   var forceMount: Boolean?

   /**
    * The preferred side of the anchor to render against when open. Will be reversed when
    * collisions occur and avoidCollisions is enabled.
    */
   var side: String? // "top", "right", "bottom", "left"

   /**
    * The distance in pixels from the anchor.
    */
   var sideOffset: Int?

   /**
    * The preferred alignment against the anchor. May change when collisions occur.
    */
   var align: String? // "start", "center", "end"

   /**
    * An offset in pixels from the "start" or "end" alignment options.
    */
   var alignOffset: Int?

   /**
    * When true, overrides the side andalign preferences to prevent collisions with boundary edges.
    */
   var avoidCollisions: Boolean?

   /**
    * The element used as the collision boundary. By default, this is the viewport, though you can
    * provide additional element(s) to be included in this check.
    *
    * Element | null | Array<Element | null>
    */
   var collisionBoundary: CollisionBoundary? // Element | null | Array<Element | null>

   /**
    * The distance in pixels from the boundary edges where collision detection should occur.
    * Accepts a number (same for all sides), or a partial padding object, for example:
    *
    * { top: 20, left: 20, bottom: 20, right: 20 }
    *
    * unsafeJso {
    *    top = 20
    *    left = 20
    *    bottom = 20
    *    right = 20
    * }
    *
    * number | Partial<Record<Side, number>>
    */
   var collisionPadding: CollisionPadding? // number | Partial<Record<Side, number>>

   /**
    * The padding between the arrow and the edges of the content. If your content
    * has border-radius, this will prevent it from overflowing the corners.
    */
   var arrowPadding: Int?

   /**
    * The sticky behavior on the align axis. "partial" will keep the content in the boundary as
    * long as the trigger is at least partially in the boundary whilst "always" will keep the
    * content in the boundary regardless.
    */
   var sticky: String? // "partial" | "always"

   /**
    * Whether to hide the content when the trigger becomes fully occluded.
    */
   var hideWhenDetached: Boolean?

   @JsName("data-state")
   var dataState: String? // "open" | "closed"

   @JsName("data-side")
   var dataSide: String? // "left" | "right" | "bottom" | "top"

   @JsName("data-align")
   var dataAlign: String? // "start" | "end" | "center"
}

/*
CSS Variable	                           Description
--radix-popover-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-popover-content-available-width	The remaining width between the trigger and the boundary edge
--radix-popover-content-available-height	The remaining height between the trigger and the boundary edge
--radix-popover-trigger-width	            The width of the trigger
--radix-popover-trigger-height            The height of the trigger
*/

/**
 * The component that pops out when the popover is open.
 */
@JsName("Content")
external val PopoverPrimitiveContent: ComponentType<PopoverContentProps>

/**
 * @see PopoverPrimitiveArrow
 */
external interface PopoverArrowProps : RadixProps, PropsWithAsChild {
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
 * An optional arrow element to render alongside the popover. This can be used to help visually
 * link the anchor with the PopoverContent. Must be rendered inside Popover.Content.
 */
@JsName("Arrow")
external val PopoverPrimitiveArrow: ComponentType<PopoverArrowProps>

/**
 * @see PopoverPrimitiveClose
 */
external interface PopoverCloseProps : RadixProps, PropsWithAsChild

/**
 * The button that closes an open popover.
 */
@JsName("Close")
external val PopoverPrimitiveClose: ComponentType<PopoverCloseProps>
