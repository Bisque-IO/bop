@file:JsModule("@radix-ui/react-navigation-menu")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyNavigationMenu = FC {
    NavigationMenu {
        NavigationMenuList {
            NavigationMenuItem {
                NavigationMenuTrigger {
                    +"Products"
                }
                NavigationMenuContent {
                    div {
                        +"Product A"
                        br()
                        +"Product B"
                    }
                }
            }

            NavigationMenuItem {
                NavigationMenuLink {
                    href = "#about"
                    +"About"
                }
            }
        }

        NavigationMenuViewport {
            className = ClassName("absolute top-full left-0 w-full bg-white shadow")
        }

        NavigationMenuIndicator {
            className = ClassName("absolute top-full h-2 w-2 bg-black")
        }
    }
}

*/

// ------------------------------
// Root
external interface NavigationMenuProps : DefaultProps {
    var orientation: String? // "horizontal" | "vertical"
    var dir: String?         // "ltr" | "rtl"
    var delayDuration: Int?
    var skipDelayDuration: Int?
    var value: String?
    var defaultValue: String?
    var onValueChange: ((String) -> Unit)?
    var asChild: Boolean?
}

@JsName("Root")
external val NavigationMenu: ComponentType<NavigationMenuProps>

// ------------------------------
// List
external interface NavigationMenuListProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("List")
external val NavigationMenuList: ComponentType<NavigationMenuListProps>

// ------------------------------
// Item
external interface NavigationMenuItemProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Item")
external val NavigationMenuItem: ComponentType<NavigationMenuItemProps>

// ------------------------------
// Link
external interface NavigationMenuLinkProps : DefaultProps {
    var active: Boolean?
    var asChild: Boolean?
}

@JsName("Link")
external val NavigationMenuLink: ComponentType<NavigationMenuLinkProps>

// ------------------------------
// Trigger
external interface NavigationMenuTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val NavigationMenuTrigger: ComponentType<NavigationMenuTriggerProps>

// ------------------------------
// Content
external interface NavigationMenuContentProps : DefaultProps {
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("Content")
external val NavigationMenuContent: ComponentType<NavigationMenuContentProps>

// ------------------------------
// Viewport
external interface NavigationMenuViewportProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Viewport")
external val NavigationMenuViewport: ComponentType<NavigationMenuViewportProps>

// ------------------------------
// Indicator
external interface NavigationMenuIndicatorProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Indicator")
external val NavigationMenuIndicator: ComponentType<NavigationMenuIndicatorProps>

// ------------------------------
// Sub (alias of Item)
@JsName("Sub")
external val NavigationMenuSub: ComponentType<DefaultProps>

// ------------------------------
// SubContent (alias of Content)
external interface NavigationMenuSubContentProps : DefaultProps {
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("SubContent")
external val NavigationMenuSubContent: ComponentType<NavigationMenuSubContentProps>
