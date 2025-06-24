@file:JsModule("@radix-ui/react-scroll-area")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

Anatomy
Import all parts and piece them together.

import { ScrollArea } from "radix-ui";

export default () => (
	<ScrollArea.Root>
		<ScrollArea.Viewport />
		<ScrollArea.Scrollbar orientation="horizontal">
			<ScrollArea.Thumb />
		</ScrollArea.Scrollbar>
		<ScrollArea.Scrollbar orientation="vertical">
			<ScrollArea.Thumb />
		</ScrollArea.Scrollbar>
		<ScrollArea.Corner />
	</ScrollArea.Root>
);

Accessibility
In most cases, it's best to rely on native scrolling and work with the customization
options available in CSS. When that isn't enough, ScrollArea provides additional
customizability while maintaining the browser's native scroll behavior (as well as
accessibility features, like keyboard scrolling).

Keyboard Interactions
Scrolling via keyboard is supported by default because the component relies on native
scrolling. Specific keyboard interactions may differ between platforms, so we do not
specify them here or add specific event listeners to handle scrolling via key events.

*/

/**
 * @see ScrollAreaPrimitiveRoot
 */
external interface ScrollAreaRootProps : RadixProps, PropsWithAsChild {
    /**
     * Describes the nature of scrollbar visibility, similar to how the scrollbar
     * preferences in macOS control visibility of native scrollbars.
     *
     * "auto" means that scrollbars are visible when content is overflowing on the
     * corresponding orientation.
     *
     * "always" means that scrollbars are always visible regardless of whether the
     * content is overflowing.
     *
     * "scroll" means that scrollbars are visible when the user is scrolling along
     * its corresponding orientation.
     *
     * "hover" when the user is scrolling along its corresponding orientation and
     * when the user is hovering over the scroll area.
     */
    var type: String? // "auto", "always", "scroll", "hover"

    /**
     * If type is set to either "scroll" or "hover", this prop determines the length
     * of time, in milliseconds, before the scrollbars are hidden after the user
     * stops interacting with scrollbars.
     */
    var scrollHideDelay: Int?

    /**
     * The reading direction of the scroll area. If omitted, inherits globally from
     * DirectionProvider or assumes LTR (left-to-right) reading mode.
     */
    var dir: String? // "ltr" | "rtl"

    /**
     * An optional nonce attribute that is passed to the inline styles for use in
     * CSP-enabled environments that use strict rules to enhance security.
     */
    var nonce: String?
}

/**
 * Contains all the parts of a scroll area.
 */
@JsName("Root")
external val ScrollAreaPrimitiveRoot: ComponentType<ScrollAreaRootProps>

/**
 * @see ScrollAreaPrimitiveViewport
 */
external interface ScrollAreaViewportProps : RadixProps, PropsWithAsChild

/**
 * The viewport area of the scroll area.
 */
@JsName("Viewport")
external val ScrollAreaPrimitiveViewport: ComponentType<ScrollAreaViewportProps>

/**
 * @see ScrollAreaPrimitiveScrollbar
 */
external interface ScrollAreaScrollbarProps : RadixProps, PropsWithAsChild {
    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries.
     */
    var forceMount: Boolean?

    /**
     * The orientation of the scrollbar.
     *
     * "horizontal" | "vertical"
     */
    var orientation: String? // "horizontal" | "vertical"

    @JsName("data-state")
    var dataState: String? // "visible" | "hidden"

    @JsName("data-orientation")
    var dataOrientation: String? // "horizontal" | "vertical"
}

/**
 * The vertical scrollbar. Add a second Scrollbar with an orientation prop to enable horizontal scrolling.
 */
@JsName("Scrollbar")
external val ScrollAreaPrimitiveScrollbar: ComponentType<ScrollAreaScrollbarProps>

/**
 * @see ScrollAreaPrimitiveThumb
 */
external interface ScrollAreaThumbProps : RadixProps, PropsWithAsChild {
    @JsName("data-state")
    var dataState: String? // "visible" | "hidden"
}

/**
 * The thumb to be used in ScrollAreaScrollbar.
 */
@JsName("Thumb")
external val ScrollAreaPrimitiveThumb: ComponentType<ScrollAreaThumbProps>

/**
 * @see ScrollAreaPrimitiveCorner
 */
external interface ScrollAreaCornerProps : RadixProps, PropsWithAsChild

/**
 * The corner where both vertical and horizontal scrollbars meet.
 */
@JsName("Corner")
external val ScrollAreaPrimitiveCorner: ComponentType<ScrollAreaCornerProps>
