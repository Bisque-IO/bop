@file:JsModule("@radix-ui/react-tabs")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyTabs = FC {
    TabsRoot {
        defaultValue = "account"

        TabsList {
            TabsTrigger {
                value = "account"
                +"Account"
            }
            TabsTrigger {
                value = "password"
                +"Password"
            }
        }

        TabsContent {
            value = "account"
            div { +"Account Settings" }
        }

        TabsContent {
            value = "password"
            div { +"Password Settings" }
        }
    }
}

*/

// ------------------------------
// Root
// ------------------------------
external interface TabsRootProps : DefaultProps {
    var defaultValue: String?
    var value: String?
    var onValueChange: ((String) -> Unit)?
    var orientation: String? // "horizontal" | "vertical"
    var dir: String?         // "ltr" | "rtl"
    var activationMode: String? // "automatic" | "manual"
}

@JsName("Root")
external val TabsRoot: ComponentType<TabsRootProps>

// ------------------------------
// List
// ------------------------------
external interface TabsListProps : DefaultProps {
    var asChild: Boolean?
    var ariaLabel: String?
}

@JsName("List")
external val TabsList: ComponentType<TabsListProps>

// ------------------------------
// Trigger
// ------------------------------
external interface TabsTriggerProps : DefaultProps {
    var value: String
    var asChild: Boolean?
    var disabled: Boolean?
}

@JsName("Trigger")
external val TabsTrigger: ComponentType<TabsTriggerProps>

// ------------------------------
// Content
// ------------------------------
external interface TabsContentProps : DefaultProps {
    var value: String
    var asChild: Boolean?
    var forceMount: Boolean?
}

@JsName("Content")
external val TabsContent: ComponentType<TabsContentProps>