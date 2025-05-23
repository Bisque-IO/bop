package bop.ui

import react.FC
import react.dom.html.ReactHTML.div
import lib.vaul.DrawerCloseProps
import lib.vaul.DrawerContentProps
import lib.vaul.DrawerDescriptionProps
import lib.vaul.DrawerOverlayProps
import lib.vaul.DrawerRootProps
import lib.vaul.DrawerTitleProps
import lib.vaul.DrawerTriggerProps
import web.cssom.ClassName

typealias DrawerDirection = lib.vaul.DrawerDirection

val Drawer = FC<DrawerRootProps>("Drawer") { props ->
   lib.vaul.Drawer.Root {
      +props
      dataSlot = "drawer"
   }
}

val DrawerTrigger = FC<DrawerTriggerProps>("DrawerTrigger") { props ->
   lib.vaul.Drawer.Trigger {
      +props
      dataSlot = "drawer-trigger"
   }
}

val DrawerPortal = FC<DrawerRootProps>("DrawerPortal") { props ->
   lib.vaul.Drawer.Portal {
      +props
      dataSlot = "drawer-portal"
   }
}

val DrawerClose = FC<DrawerCloseProps>("DrawerClose") { props ->
   lib.vaul.Drawer.Close {
      +props
      dataSlot = "drawer-close"
   }
}

val DrawerOverlay = FC<DrawerOverlayProps>("DrawerOverlay") { props ->
   lib.vaul.Drawer.Overlay {
      +props
      dataSlot = "drawer-overlay"
      className = cn(
         "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50 bg-black/50",
         props.className
      )
   }
}

val DrawerContent = FC<DrawerContentProps>("DrawerContent") { props ->
   DrawerPortal {
      DrawerOverlay {
         lib.vaul.Drawer.Content {
            +props
            children = null
            dataSlot = "drawer-content"
            className = cn(
               "group/drawer-content bg-background fixed z-50 flex h-auto flex-col",
               "data-[vaul-drawer-direction=top]:inset-x-0 data-[vaul-drawer-direction=top]:top-0 data-[vaul-drawer-direction=top]:mb-24 data-[vaul-drawer-direction=top]:max-h-[80vh] data-[vaul-drawer-direction=top]:rounded-b-lg data-[vaul-drawer-direction=top]:border-b",
               "data-[vaul-drawer-direction=bottom]:inset-x-0 data-[vaul-drawer-direction=bottom]:bottom-0 data-[vaul-drawer-direction=bottom]:mt-24 data-[vaul-drawer-direction=bottom]:max-h-[80vh] data-[vaul-drawer-direction=bottom]:rounded-t-lg data-[vaul-drawer-direction=bottom]:border-t",
               "data-[vaul-drawer-direction=right]:inset-y-0 data-[vaul-drawer-direction=right]:right-0 data-[vaul-drawer-direction=right]:w-3/4 data-[vaul-drawer-direction=right]:border-l data-[vaul-drawer-direction=right]:sm:max-w-sm",
               "data-[vaul-drawer-direction=left]:inset-y-0 data-[vaul-drawer-direction=left]:left-0 data-[vaul-drawer-direction=left]:w-3/4 data-[vaul-drawer-direction=left]:border-r data-[vaul-drawer-direction=left]:sm:max-w-sm",
               props.className
            )

            div {
               className =
                  ClassName("bg-muted mx-auto mt-4 hidden h-2 w-[100px] shrink-0 rounded-full group-data-[vaul-drawer-direction=bottom]/drawer-content:block")
            }

            +props.children
         }
      }
   }
}

val DrawerHeader = FC<DefaultProps>("DrawerHeader") { props ->
   div {
      +props
      dataSlot = "drawer-header"
      className = cn("flex flex-col gap-1.5 p-4", props.className)
   }
}

val DrawerFooter = FC<DefaultProps>("DrawerFooter") { props ->
   div {
      +props
      dataSlot = "drawer-footer"
      className = cn("mt-auto flex flex-col gap-2 p-4", props.className)
   }
}

val DrawerTitle = FC<DrawerTitleProps>("DrawerTitle") { props ->
   lib.vaul.Drawer.Title {
      +props
      dataSlot = "drawer-title"
      className = cn("text-foreground font-semibold", props.className)
   }
}

val DrawerDescription = FC<DrawerDescriptionProps>("DrawerDescription") { props ->
   lib.vaul.Drawer.Description {
      +props
      dataSlot = "drawer-description"
      className = cn("text-muted-foreground text-sm", props.className)
   }
}
