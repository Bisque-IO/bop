@file:JsModule("@radix-ui/react-navigation-menu")
@file:JsNonModule

package lib.radix

import react.ComponentType
import web.events.Event
import web.uievents.KeyboardEvent

/*
Anatomy
Import all parts and piece them together.

import { NavigationMenu } from "radix-ui";

export default () => (
	<NavigationMenu.Root>
		<NavigationMenu.List>
			<NavigationMenu.Item>
				<NavigationMenu.Trigger />
				<NavigationMenu.Content>
					<NavigationMenu..Link />
				</NavigationMenu.Content>
			</NavigationMenu.Item>

			<NavigationMenu.Item>
				<NavigationMenu..Link />
			</NavigationMenu.Item>

			<NavigationMenu.Item>
				<NavigationMenu.Trigger />
				<NavigationMenu.Content>
					<NavigationMenu.Sub>
						<NavigationMenu.List />
						<NavigationMenu.Viewport />
					</NavigationMenu.Sub>
				</NavigationMenu.Content>
			</NavigationMenu.Item>

			<NavigationMenu.Indicator />
		</NavigationMenu.List>

		<NavigationMenu.Viewport />
	</NavigationMenu.Root>
);

Examples
Vertical
You can create a vertical menu by using the orientation prop.

<NavigationMenu.Root orientation="vertical">
	<NavigationMenu.List>
		<NavigationMenu.Item>
			<NavigationMenu.Trigger>Item one</NavigationMenu.Trigger>
			<NavigationMenu.Content>Item one content</NavigationMenu.Content>
		</NavigationMenu.Item>
		<NavigationMenu.Item>
			<NavigationMenu.Trigger>Item two</NavigationMenu.Trigger>
			<NavigationMenu.Content>Item Two content</NavigationMenu.Content>
		</NavigationMenu.Item>
	</NavigationMenu.List>
</NavigationMenu.Root>

Flexible layouts
Use the Viewport part when you need extra control over where Content is rendered.
This can be helpful when your design requires an adjusted DOM structure or if you
need flexibility to achieve advanced animation. Tab focus will be maintained
automatically.

<NavigationMenu.Root>
	<NavigationMenu.List>
		<NavigationMenu.Item>
			<NavigationMenu.Trigger>Item one</NavigationMenu.Trigger>
			<NavigationMenu.Content>Item one content</NavigationMenu.Content>
		</NavigationMenu.Item>
		<NavigationMenu.Item>
			<NavigationMenu.Trigger>Item two</NavigationMenu.Trigger>
			<NavigationMenu.Content>Item two content</NavigationMenu.Content>
		</NavigationMenu.Item>
	</NavigationMenu.List>

	{/* NavigationMenu.Content will be rendered here when active */}
	<NavigationMenu.Viewport />
</NavigationMenu.Root>

With indicator
You can use the optional Indicator part to highlight the currently active
Trigger, this is useful when you want to provide an animated visual cue
such as an arrow or highlight to accompany the Viewport.

// index.jsx
import { NavigationMenu } from "radix-ui";
import "./styles.css";

export default () => (
	<NavigationMenu.Root>
		<NavigationMenu.List>
			<NavigationMenu.Item>
				<NavigationMenu.Trigger>Item one</NavigationMenu.Trigger>
				<NavigationMenu.Content>Item one content</NavigationMenu.Content>
			</NavigationMenu.Item>
			<NavigationMenu.Item>
				<NavigationMenu.Trigger>Item two</NavigationMenu.Trigger>
				<NavigationMenu.Content>Item two content</NavigationMenu.Content>
			</NavigationMenu.Item>

			<NavigationMenu.Indicator className="NavigationMenuIndicator" />
		</NavigationMenu.List>

		<NavigationMenu.Viewport />
	</NavigationMenu.Root>
);
/* styles.css */
.NavigationMenuIndicator {
	background-color: grey;
}
.NavigationMenuIndicator[data-orientation="horizontal"] {
	height: 3px;
	transition:
		width,
		transform,
		250ms ease;
}

With submenus
Create a submenu by nesting your NavigationMenu and using the Sub part in place of its Root.
Submenus work differently to Root navigation menus and are similar to Tabs in that one item
should always be active, so be sure to assign and set a defaultValue.

<NavigationMenu.Root>
	<NavigationMenu.List>
		<NavigationMenu.Item>
			<NavigationMenu.Trigger>Item one</NavigationMenu.Trigger>
			<NavigationMenu.Content>Item one content</NavigationMenu.Content>
		</NavigationMenu.Item>
		<NavigationMenu.Item>
			<NavigationMenu.Trigger>Item two</NavigationMenu.Trigger>
			<NavigationMenu.Content>
				<NavigationMenu.Sub defaultValue="sub1">
					<NavigationMenu.List>
						<NavigationMenu.Item value="sub1">
							<NavigationMenu.Trigger>Sub item one</NavigationMenu.Trigger>
							<NavigationMenu.Content>
								Sub item one content
							</NavigationMenu.Content>
						</NavigationMenu.Item>
						<NavigationMenu.Item value="sub2">
							<NavigationMenu.Trigger>Sub item two</NavigationMenu.Trigger>
							<NavigationMenu.Content>
								Sub item two content
							</NavigationMenu.Content>
						</NavigationMenu.Item>
					</NavigationMenu.List>
				</NavigationMenu.Sub>
			</NavigationMenu.Content>
		</NavigationMenu.Item>
	</NavigationMenu.List>
</NavigationMenu.Root>

With client side routing
If you need to use the .Link component provided by your routing package then we recommend composing
with NavigationMenu..Link via a custom component. This will ensure accessibility and consistent
keyboard control is maintained. Here's an example using Next.js:

// index.jsx
import { usePathname } from "next/navigation";
import NextLink from "next/link";
import { NavigationMenu } from "radix-ui";
import "./styles.css";

const .Link = ({ href, ...props }) => {
	const pathname = usePathname();
	const isActive = href === pathname;

	return (
		<NavigationMenu..Link asChild active={isActive}>
			<NextLink href={href} className="NavigationMenuLink" {...props} />
		</NavigationMenu..Link>
	);
};

export default () => (
	<NavigationMenu.Root>
		<NavigationMenu.List>
			<NavigationMenu.Item>
				<.Link href="/">.Home</.Link>
			</NavigationMenu.Item>
			<NavigationMenu.Item>
				<.Link href="/about">.About</.Link>
			</NavigationMenu.Item>
		</NavigationMenu.List>
	</NavigationMenu.Root>
);
/* styles.css */
.NavigationMenuLink {
	text-decoration: none;
}
.NavigationMenuLink[data-active] {
	text-decoration: "underline";
}

Advanced animation
We expose --radix-navigation-menu-viewport-[width|height] and
data-motion['from-start'|'to-start'|'from-end'|'to-end']
attributes to allow you to animate Viewport size and Content position based on the enter/exit direction.

Combining these with position: absolute; allows you to create smooth overlapping
animation effects when moving between items.

// index.jsx
import { NavigationMenu } from "radix-ui";
import "./styles.css";

export default () => (
	<NavigationMenu.Root>
		<NavigationMenu.List>
			<NavigationMenu.Item>
				<NavigationMenu.Trigger>Item one</NavigationMenu.Trigger>
				<NavigationMenu.Content className="NavigationMenuContent">
					Item one content
				</NavigationMenu.Content>
			</NavigationMenu.Item>
			<NavigationMenu.Item>
				<NavigationMenu.Trigger>Item two</NavigationMenu.Trigger>
				<NavigationMenu.Content className="NavigationMenuContent">
					Item two content
				</NavigationMenu.Content>
			</NavigationMenu.Item>
		</NavigationMenu.List>

		<NavigationMenu.Viewport className="NavigationMenuViewport" />
	</NavigationMenu.Root>
);
/* styles.css */
.NavigationMenuContent {
	position: absolute;
	top: 0;
	left: 0;
	animation-duration: 250ms;
	animation-timing-function: ease;
}
.NavigationMenuContent[data-motion="from-start"] {
	animation-name: enterFromLeft;
}
.NavigationMenuContent[data-motion="from-end"] {
	animation-name: enterFromRight;
}
.NavigationMenuContent[data-motion="to-start"] {
	animation-name: exitToLeft;
}
.NavigationMenuContent[data-motion="to-end"] {
	animation-name: exitToRight;
}

.NavigationMenuViewport {
	position: relative;
	width: var(--radix-navigation-menu-viewport-width);
	height: var(--radix-navigation-menu-viewport-height);
	transition:
		width,
		height,
		250ms ease;
}

@keyframes enterFromRight {
	from {
		opacity: 0;
		transform: translateX(200px);
	}
	to {
		opacity: 1;
		transform: translateX(0);
	}
}

@keyframes enterFromLeft {
	from {
		opacity: 0;
		transform: translateX(-200px);
	}
	to {
		opacity: 1;
		transform: translateX(0);
	}
}

@keyframes exitToRight {
	from {
		opacity: 1;
		transform: translateX(0);
	}
	to {
		opacity: 0;
		transform: translateX(200px);
	}
}

@keyframes exitToLeft {
	from {
		opacity: 1;
		transform: translateX(0);
	}
	to {
		opacity: 0;
		transform: translateX(-200px);
	}
}


Accessibility
Adheres to the navigation role requirements.

Differences to menubar
NavigationMenu should not be confused with menubar, although this primitive shares the name menu in the
colloquial sense to refer to a set of navigation links, it does not use the WAI-ARIA menu role. This is
because menu and menubars behave like native operating system menus most commonly found in desktop
application windows, as such they feature complex functionality like composite focus management and
first-character navigation.

These features are often considered unnecessary for website navigation and at worst can confuse users
who are familiar with established website patterns.

See the W3C Disclosure Navigation Menu example for more information.

.Link usage and aria-current
It's important to use NavigationMenu..Link for all navigational links within a menu, this not only applies
to the bop.demo.main list but also within any content rendered via NavigationMenu.Content. This will ensure consistent
keyboard interactions and accessibility while also giving access to the active prop for setting aria-current
and the active styles. See this example for more information on usage with third party routing .components.

Keyboard Interactions
Key	                      Description
Space Enter                 When focus is on NavMenuTrigger, opens the content.
Tab                         Moves focus to the next focusable element.
ArrowDown                   When horizontal and focus is on an open NavMenu.Trigger, moves focus into NavMenuContent.
                            Moves focus to the next NavMenuTrigger or NavMenuLink.
ArrowUp                     Moves focus to the previous NavMenuTrigger or NavMenuLink.
ArrowRight ArrowLeft        When vertical and focus is on an open NavMenuTrigger, moves focus into its NavMenuContent.
                            Moves focus to the next / previous NavMenuTrigger or NavMenuLink.
.Home End                    Moves focus to the first/last NavMenuTrigger or NavMenuLink.
Esc                         Closes open NavMenuContent and moves focus to its NavMenuTrigger.
*/

/**
 * @see NavMenuPrimitiveRoot
 */
external interface NavMenuRootProps : RadixProps {
    /**
     * The value of the menu item that should be active when initially rendered.
     * Use when you do not need to control the value state.
     */
    var defaultValue: String?

    /**
     * The controlled value of the menu item to activate.
     * Should be used in conjunction with onValueChange.
     */
    var value: String?

    /**
     * Event handler called when the value changes.
     */
    var onValueChange: ((value: String) -> Unit)?

    /**
     * The duration from when the mouse enters a trigger until the content opens.
     */
    var delayDuration: Int?

    /**
     * How much time a user has to enter another trigger without incurring a delay again.
     */
    var skipDelayDuration: Int?

    /**
     * The reading direction of the menu when applicable. If omitted, inherits globally
     * from DirectionProvider or assumes LTR (left-to-right) reading mode.
     */
    var dir: String? // "ltr" | "rtl"

    /**
     * The orientation of the menu.
     */
    var orientation: String? // "horizontal" | "vertical"

    @JsName("data-orientation")
    var dataOrientation: String? // "horizontal" | "vertical"
}

/**
 * Contains all the parts of a navigation menu.
 */
@JsName("Root")
external val NavMenuPrimitiveRoot: ComponentType<NavMenuRootProps>

/**
 * @see NavMenuPrimitiveSub
 */
external interface NavMenuSubProps : RadixProps {
    /**
     * The value of the menu item that should be active when initially rendered.
     * Use when you do not need to control the value state.
     */
    var defaultValue: String?

    /**
     * The controlled value of the sub menu item to activate.
     * Should be used in conjunction with onValueChange.
     */
    var value: String?

    /**
     * Event handler called when the value changes.
     */
    var onValueChange: ((value: String) -> Unit)?

    /**
     * The orientation of the menu.
     */
    var orientation: String? // "horizontal" | "vertical"

    @JsName("data-orientation")
    var dataOrientation: String? // "horizontal" | "vertical"
}

/**
 * Signifies a submenu. Use it in place of the root part when nested to create a submenu.
 */
@JsName("Sub")
external val NavMenuPrimitiveSub: ComponentType<NavMenuSubProps>

/**
 * @see NavMenuPrimitiveList
 */
external interface NavMenuListProps : RadixProps, PropsWithAsChild {
    @JsName("data-orientation")
    var dataOrientation: String? // "horizontal" | "vertical"
}

/**
 * Contains the top level menu items.
 */
@JsName("List")
external val NavMenuPrimitiveList: ComponentType<NavMenuListProps>

/**
 * @see NavMenuPrimitiveItem
 */
external interface NavMenuItemProps : RadixProps, PropsWithAsChild {
    /**
     * A unique value that associates the item with an active value when the
     * navigation menu is controlled. This prop is managed automatically when
     * uncontrolled.
     */
    var value: String?
}

/**
 * A top level menu item, contains a link or trigger with content combination.
 */
@JsName("Item")
external val NavMenuPrimitiveItem: ComponentType<NavMenuItemProps>

/**
 * @see NavMenuPrimitiveTrigger
 */
external interface NavMenuTriggerProps : RadixProps, PropsWithAsChild {
    @JsName("data-state")
    var dataState: String? // "open" | "closed"

    @JsName("data-disabled")
    var dataDisabled: Boolean?
}

/**
 * The button that toggles the content.
 */
@JsName("Trigger")
external val NavMenuPrimitiveTrigger: ComponentType<NavMenuTriggerProps>

/**
 * @see NavMenuPrimitiveContent
 */
external interface NavMenuContentProps : RadixProps, PropsWithAsChild {
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
     * Event handler called when focus moves outside the bounds of the component.
     * It can be prevented by calling event.preventDefault.
     */
    var onFocusOutside: ((Event) -> Unit)?

    /**
     * Event handler called when an interaction (pointer or focus event) happens
     * outside the bounds of the component. It can be prevented by calling
     * event.preventDefault.
     */
    var onInteractOutside: ((Event) -> Unit)?

    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries.
     */
    var forceMount: Boolean?

    @JsName("data-state")
    var dataState: String? // "open" | "closed"

    @JsName("data-motion")
    var dataMotion: String? // "to-start" | "to-end" | "from-start" | "from-end"

    @JsName("data-orientation")
    var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * Contains the content associated with each trigger.
 */
@JsName("Content")
external val NavMenuPrimitiveContent: ComponentType<NavMenuContentProps>

/**
 * @see NavMenuPrimitiveLink
 */
external interface NavMenuLinkProps : RadixProps, PropsWithAsChild {
    /**
     * Used to identify the link as the currently active page.
     */
    var active: Boolean?

    /**
     * Event handler called when the user selects a link (via mouse or keyboard).
     * Calling event.preventDefault in this handler will prevent the navigation
     * menu from closing when selecting that link.
     */
    var onSelect: ((event: Event) -> Unit)?

    @JsName("data-active")
    var dataActive: Boolean?
}

/**
 * A navigational link.
 */
@JsName("Link")
external val NavMenuPrimitiveLink: ComponentType<NavMenuLinkProps>

/**
 * @see NavMenuPrimitiveIndicator
 */
external interface NavMenuIndicatorProps : RadixProps, PropsWithAsChild {
    /**
     * Used to force mounting when more control is needed. Useful when
     * controlling animation with React animation libraries.
     */
    var forceMount: Boolean?

    @JsName("data-state")
    var dataState: String? // "visible" | "hidden"

    @JsName("data-orientation")
    var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * An optional indicator element that renders below the list,
 * is used to highlight the currently active trigger.
 */
@JsName("Indicator")
external val NavMenuPrimitiveIndicator: ComponentType<NavMenuIndicatorProps>

/**
 * @see NavMenuPrimitiveViewport
 */
external interface NavMenuViewportProps : RadixProps, PropsWithAsChild {
    /**
     * Used to force mounting when more control is needed. Useful when controlling
     * animation with React animation libraries.
     */
    var forceMount: Boolean?

    @JsName("data-state")
    var dataState: String? // "visible" | "hidden"

    @JsName("data-orientation")
    var dataOrientation: String? // "vertical" | "horizontal"
}

/**
 * An optional viewport element that is used to render active content outside the list.
 */
@JsName("Viewport")
external val NavMenuPrimitiveViewport: ComponentType<NavMenuViewportProps>

