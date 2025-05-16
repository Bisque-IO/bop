package bop.ui

import react.FC
import react.dom.html.ReactHTML.div
import vaul.DrawerCloseProps
import vaul.DrawerContentProps
import vaul.DrawerDescriptionProps
import vaul.DrawerOverlayProps
import vaul.DrawerRootProps
import vaul.DrawerTitleProps
import vaul.DrawerTriggerProps
import web.cssom.ClassName

val Drawer = FC<DrawerRootProps>("Drawer") { props ->
   vaul.Drawer.Root {
      dataSlot = "drawer"
      spread(props)
   }
}

val DrawerTrigger = FC<DrawerTriggerProps>("DrawerTrigger") { props ->
   vaul.Drawer.Trigger {
      dataSlot = "drawer-trigger"
      spread(props)
   }
}

val DrawerPortal = FC<DrawerRootProps>("DrawerPortal") { props ->
   vaul.Drawer.Portal {
      dataSlot = "drawer-portal"
      spread(props)
   }
}

val DrawerClose = FC<DrawerCloseProps>("DrawerClose") { props ->
   vaul.Drawer.Close {
      dataSlot = "drawer-close"
      spread(props)
   }
}

val DrawerOverlay = FC<DrawerOverlayProps>("DrawerOverlay") { props ->
   vaul.Drawer.Overlay {
      dataSlot = "drawer-overlay"
      className = cn("data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50 bg-black/50", props.className)
      spread(props, "className")
   }
}

val DrawerContent = FC<DrawerContentProps>("DrawerContent") { props ->
   DrawerPortal {
      DrawerOverlay {
         vaul.Drawer.Content {
            dataSlot = "drawer-content"
            className = cn(
               "group/drawer-content bg-background fixed z-50 flex h-auto flex-col",
               "data-[vaul-drawer-direction=top]:inset-x-0 data-[vaul-drawer-direction=top]:top-0 data-[vaul-drawer-direction=top]:mb-24 data-[vaul-drawer-direction=top]:max-h-[80vh] data-[vaul-drawer-direction=top]:rounded-b-lg data-[vaul-drawer-direction=top]:border-b",
               "data-[vaul-drawer-direction=bottom]:inset-x-0 data-[vaul-drawer-direction=bottom]:bottom-0 data-[vaul-drawer-direction=bottom]:mt-24 data-[vaul-drawer-direction=bottom]:max-h-[80vh] data-[vaul-drawer-direction=bottom]:rounded-t-lg data-[vaul-drawer-direction=bottom]:border-t",
               "data-[vaul-drawer-direction=right]:inset-y-0 data-[vaul-drawer-direction=right]:right-0 data-[vaul-drawer-direction=right]:w-3/4 data-[vaul-drawer-direction=right]:border-l data-[vaul-drawer-direction=right]:sm:max-w-sm",
               "data-[vaul-drawer-direction=left]:inset-y-0 data-[vaul-drawer-direction=left]:left-0 data-[vaul-drawer-direction=left]:w-3/4 data-[vaul-drawer-direction=left]:border-r data-[vaul-drawer-direction=left]:sm:max-w-sm",
               props.className
            )
            spread(props, "className", "children")

            div {
               className = ClassName("bg-muted mx-auto mt-4 hidden h-2 w-[100px] shrink-0 rounded-full group-data-[vaul-drawer-direction=bottom]/drawer-content:block")
            }

            +props.children
         }
      }
   }
}

val DrawerHeader = FC<DefaultProps>("DrawerHeader") { props ->
   div {
      dataSlot = "drawer-header"
      className = cn("flex flex-col gap-1.5 p-4", props.className)
      spread(props, "className")
   }
}

val DrawerFooter = FC<DefaultProps>("DrawerFooter") { props ->
   div {
      dataSlot = "drawer-footer"
      className = cn("mt-auto flex flex-col gap-2 p-4", props.className)
      spread(props, "className")
   }
}

val DrawerTitle = FC<DrawerTitleProps>("DrawerTitle") { props ->
   vaul.Drawer.Title {
      dataSlot = "drawer-title"
      className = cn("text-foreground font-semibold", props.className)
      spread(props, "className")
   }
}

val DrawerDescription = FC<DrawerDescriptionProps>("DrawerDescription") { props ->
   vaul.Drawer.Description {
      dataSlot = "drawer-description"
      className = cn("text-muted-foreground text-sm", props.className)
      spread(props, "className")
   }
}
