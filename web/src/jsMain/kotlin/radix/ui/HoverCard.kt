@file:JsModule("@radix-ui/react-hover-card")
@file:JsNonModule

package radix.ui

import react.ComponentType
import web.html.HTMLElement

/*

Anatomy

Import all parts and piece them together.

import { HoverCard } from "radix-ui";

export default () => (
	<HoverCard.Root>
		<HoverCard.Trigger />
		<HoverCard.Portal>
			<HoverCard.Content>
				<HoverCard.Arrow />
			</HoverCard.Content>
		</HoverCard.Portal>
	</HoverCard.Root>
);


Examples

Show instantly
Use the openDelay prop to control the time it takes for the hover card to open.

import { HoverCard } from "radix-ui";

export default () => (
	<HoverCard.Root openDelay={0}>
		<HoverCard.Trigger>…</HoverCard.Trigger>
		<HoverCard.Content>…</HoverCard.Content>
	</HoverCard.Root>
);
Constrain the content size
You may want to constrain the width of the content so that it matches the trigger width.
You may also want to constrain its height to not exceed the viewport.

We expose several CSS custom properties such as --radix-hover-card-trigger-width and
--radix-hover-card-content-available-height to support this. Use them to constrain the content dimensions.

// index.jsx
import { HoverCard } from "radix-ui";
import "./styles.css";

export default () => (
	<HoverCard.Root>
		<HoverCard.Trigger>…</HoverCard.Trigger>
		<HoverCard.Portal>
			<HoverCard.Content className="HoverCardContent" sideOffset={5}>
				…
			</HoverCard.Content>
		</HoverCard.Portal>
	</HoverCard.Root>
);
/* styles.css */
.HoverCardContent {
	width: var(--radix-hover-card-trigger-width);
	max-height: var(--radix-hover-card-content-available-height);
}

Origin-aware animations
We expose a CSS custom property --radix-hover-card-content-transform-origin.
Use it to animate the content from its computed origin based on side, sideOffset, align, alignOffset and any collisions.

// index.jsx
import { HoverCard } from "radix-ui";
import "./styles.css";

export default () => (
	<HoverCard.Root>
		<HoverCard.Trigger>…</HoverCard.Trigger>
		<HoverCard.Content className="HoverCardContent">…</HoverCard.Content>
	</HoverCard.Root>
);
/* styles.css */
.HoverCardContent {
	transform-origin: var(--radix-hover-card-content-transform-origin);
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
import { HoverCard } from "radix-ui";
import "./styles.css";

export default () => (
	<HoverCard.Root>
		<HoverCard.Trigger>…</HoverCard.Trigger>
		<HoverCard.Content className="HoverCardContent">…</HoverCard.Content>
	</HoverCard.Root>
);
/* styles.css */
.HoverCardContent {
	animation-duration: 0.6s;
	animation-timing-function: cubic-bezier(0.16, 1, 0.3, 1);
}
.HoverCardContent[data-side="top"] {
	animation-name: slideUp;
}
.HoverCardContent[data-side="bottom"] {
	animation-name: slideDown;
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
*/

/**
 * @see HoverCardRoot
 */
external interface HoverCardRootProps : DefaultProps {
    /**
     * The open state of the hover card when it is initially rendered.
     * Use when you do not need to control its open state.
     */
    var defaultOpen: Boolean?

    /**
     * The controlled open state of the hover card. Must be used in conjunction with onOpenChange.
     */
    var open: Boolean?

    /**
     * Event handler called when the open state of the hover card changes.
     */
    var onOpenChange: ((open: Boolean) -> Unit)?

    /**
     * The duration from when the mouse enters the trigger until the hover card opens.
     */
    var openDelay: Int?

    /**
     * The duration from when the mouse leaves the trigger or content until the hover card closes.
     */
    var closeDelay: Int?
}

/**
 * Contains all the parts of a hover card.
 */
@JsName("Root")
external val HoverCardRoot: ComponentType<DefaultProps>

/**
 * @see HoverCardTrigger
 */
external interface HoverCardTriggerProps : DefaultProps, PropsWithAsChild {
    @JsName("data-state")
    var dataState: String? // "open" | "closed"
}

/**
 * The link that opens the hover card when hovered.
 */
@JsName("Trigger")
external val HoverCardTrigger: ComponentType<HoverCardTriggerProps>

/**
 * @see HoverCardPortal
 */
external interface HoverCardPortalProps : DefaultProps {
    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries. If used on this part, it will be
     * inherited by HoverCard.Content.
     */
    var forceMount: Boolean?

    /**
     * Specify a container element to portal the content into.
     */
    var container: HTMLElement? // HTMLElement or null
}

/**
 * When used, portals the content part into the body.
 */
@JsName("Portal")
external val HoverCardPortal: ComponentType<HoverCardPortalProps>

/**
 * @see HoverCardContent
 */
external interface HoverCardContentProps : DefaultProps, PropsWithAsChild {
    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries. It inherits from HoverCardPortal.
     */
    var forceMount: Boolean?

    /**
     * The preferred side of the trigger to render against when open.
     * Will be reversed when collisions occur and avoidCollisions is enabled.
     *
     * "top" | "right" | "bottom" | "left"
     */
    var side: String? // "top" | "right" | "bottom" | "left"

    /**
     * The distance in pixels from the trigger.
     */
    var sideOffset: Int?

    /**
     * The preferred alignment against the trigger. May change when collisions occur.
     *
     * "start" | "center" | "end"
     */
    var align: String? // "start" | "center" | "end"

    /**
     * An offset in pixels from the "start" or "end" alignment options.
     */
    var alignOffset: Int?

    /**
     * When true, overrides the side andalign preferences to prevent
     * collisions with boundary edges.
     */
    var avoidCollisions: Boolean?

    /**
     * The element used as the collision boundary. By default, this is the
     * viewport, though you can provide additional element(s) to be included
     * in this check.
     *
     * Element | null | Array<Element | null>
     */
    var collisionBoundary: dynamic // Element | null | Array<Element | null>

    /**
     * The distance in pixels from the boundary edges where collision detection should occur.
     * Accepts a number (same for all sides), or a partial padding object, for example:
     *
     * { top: 20, left: 20, right: 20, bottom: 20 }
     *
     * unsafeJso {
     *    top = 20
     *    left = 20
     *    right = 20
     *    bottom = 20
     * }
     */
    var collisionPadding: dynamic // number | Partial<Record<Side, number>>

    /**
     * The padding between the arrow and the edges of the content. If your content
     * has border-radius, this will prevent it from overflowing the corners.
     */
    var arrowPadding: Int?

    /**
     * Whether to hide the content when the trigger becomes fully occluded.
     */
    var hideWhenDetected: Boolean?

    @JsName("data-state")
    var dataState: String? // "open" | "closed"
    @JsName("data-side")
    var dataSide: String? // "left" | "right" | "bottom" | "top"
    @JsName("data-align")
    var dataAlign: String? // "start" | "end" | "center"
}

/*

CSS Variable	                              Description
--radix-hover-card-content-transform-origin	The transform-origin computed from the content and arrow positions/offsets
--radix-hover-card-content-available-width	The remaining width between the trigger and the boundary edge
--radix-hover-card-content-available-height	The remaining height between the trigger and the boundary edge
--radix-hover-card-trigger-width             The width of the trigger
--radix-hover-card-trigger-height	         The height of the trigger

*/

/**
 * The component that pops out when the hover card is open.
 */
@JsName("Content")
external val HoverCardContent: ComponentType<HoverCardContentProps>

/**
 * @see HoverCardArrow
 */
external interface HoverCardArrowProps : DefaultProps, PropsWithAsChild {
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
 * An optional arrow element to render alongside the hover card.
 * This can be used to help visually link the trigger with the HoverCardContent.
 * Must be rendered inside HoverCardContent.
 */
@JsName("Arrow")
external val HoverCardArrow: ComponentType<HoverCardArrowProps>
