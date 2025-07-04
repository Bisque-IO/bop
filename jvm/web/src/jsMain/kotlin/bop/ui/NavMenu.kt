package bop.ui

import js.objects.unsafeJso
import lib.cva.cva
import lib.lucide.ChevronDownIcon
import lib.radix.*
import react.FC
import react.dom.html.ReactHTML.div
import web.cssom.ClassName

external interface NavMenuProps : NavMenuRootProps {
   var viewport: Boolean?
}

val NavMenu = FC<NavMenuProps>("NavMenu") { props ->
   NavMenuPrimitiveRoot {
      +props
      children = null
      dataSlot = "navigation-menu"
      dataViewport = props.viewport ?: true
      className =
         cn("group/navigation-menu relative flex max-w-max flex-1 items-center justify-center", props.className)

      +props.children
      if (props.viewport == true) {
         NavMenuViewport {}
      }
   }
}

val NavMenuList = FC<NavMenuListProps>("NavMenuList") { props ->
   NavMenuPrimitiveList {
      +props
      dataSlot = "navigation-menu-list"
      className = cn("group flex flex-1 list-none items-center justify-center gap-1", props.className)
   }
}

val NavMenuItem = FC<NavMenuItemProps>("NavMenuItem") { props ->
   NavMenuPrimitiveItem {
      +props
      dataSlot = "navigation-menu-item"
      className = cn("relative", props.className)
   }
}

val navigationMenuTriggerStyle = cva(
   "group inline-flex h-9 w-max items-center justify-center rounded-md bg-background px-4 py-2 text-sm font-medium hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground disabled:pointer-events-none disabled:opacity-50 data-[state=open]:hover:bg-accent data-[state=open]:text-accent-foreground data-[state=open]:focus:bg-accent data-[state=open]:bg-accent/50 focus-visible:ring-ring/50 outline-none transition-[color,box-shadow] focus-visible:ring-[3px] focus-visible:outline-1",
   unsafeJso()
)

val NavMenuTrigger = FC<NavMenuTriggerProps>("NavMenuTrigger") { props ->
   NavMenuPrimitiveTrigger {
      +props
      children = null
      dataSlot = "navigation-menu-trigger"
      className = cn(navigationMenuTriggerStyle(), "group", props.className)

      +props.children
      +" "
      ChevronDownIcon {
         className =
            cn("relative top-[1px] ml-1 size-3 transition duration-300 group-data-[state=open]:rotate-180")
         ariaHidden = true
      }
   }
}

val NavMenuContent = FC<NavMenuContentProps>("NavMenuContent") { props ->
   NavMenuPrimitiveContent {
      +props
      dataSlot = "navigation-menu-content"
      className = cn(
         "z-50 data-[motion^=from-]:animate-in data-[motion^=to-]:animate-out data-[motion^=from-]:fade-in data-[motion^=to-]:fade-out data-[motion=from-end]:slide-in-from-right-52 data-[motion=from-start]:slide-in-from-left-52 data-[motion=to-end]:slide-out-to-right-52 data-[motion=to-start]:slide-out-to-left-52 top-0 left-0 w-full p-2 pr-2.5 md:absolute md:w-auto",
         "group-data-[viewport=false]/navigation-menu:bg-popover group-data-[viewport=false]/navigation-menu:text-popover-foreground group-data-[viewport=false]/navigation-menu:data-[state=open]:animate-in group-data-[viewport=false]/navigation-menu:data-[state=closed]:animate-out group-data-[viewport=false]/navigation-menu:data-[state=closed]:zoom-out-95 group-data-[viewport=false]/navigation-menu:data-[state=open]:zoom-in-95 group-data-[viewport=false]/navigation-menu:data-[state=open]:fade-in-0 group-data-[viewport=false]/navigation-menu:data-[state=closed]:fade-out-0 group-data-[viewport=false]/navigation-menu:top-full group-data-[viewport=false]/navigation-menu:mt-1.5 group-data-[viewport=false]/navigation-menu:overflow-hidden group-data-[viewport=false]/navigation-menu:rounded-md group-data-[viewport=false]/navigation-menu:border group-data-[viewport=false]/navigation-menu:shadow group-data-[viewport=false]/navigation-menu:duration-200 **:data-[slot=navigation-menu-link]:focus:ring-0 **:data-[slot=navigation-menu-link]:focus:outline-none",
         props.className
      )
   }
}

val NavMenuViewport = FC<NavMenuViewportProps>("NavMenuViewport") { props ->
   div {
      className = cn("absolute top-full left-0 isolate z-50 flex justify-center")

      NavMenuPrimitiveViewport {
         +props
         dataSlot = "navigation-menu-viewport"
         className = cn(
            "origin-top-center bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-90 relative mt-1.5 h-[var(--radix-navigation-menu-viewport-height)] w-full overflow-hidden rounded-md border shadow md:w-[var(--radix-navigation-menu-viewport-width)]",
            props.className
         )
      }
   }
}

val NavMenuLink = FC<NavMenuLinkProps>("NavMenuLink") { props ->
   NavMenuPrimitiveLink {
      +props
      dataSlot = "navigation-menu-link"
      className = cn(
         "data-[active=true]:focus:bg-accent data-[active=true]:hover:bg-accent data-[active=true]:bg-accent/50 data-[active=true]:text-accent-foreground hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground focus-visible:ring-ring/50 [&_svg:not([class*='text-'])]:text-muted-foreground flex flex-col gap-1 rounded-sm p-2 text-sm transition-all outline-none focus-visible:ring-[3px] focus-visible:outline-1 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
   }
}

val NavMenuIndicator = FC<NavMenuIndicatorProps>("NavMenuIndicator") { props ->
   NavMenuPrimitiveIndicator {
      +props
      children = null
      dataSlot = "navigation-menu-indicator"
      className = cn(
         "data-[active=true]:focus:bg-accent data-[active=true]:hover:bg-accent data-[active=true]:bg-accent/50 data-[active=true]:text-accent-foreground hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground focus-visible:ring-ring/50 [&_svg:not([class*='text-'])]:text-muted-foreground flex flex-col gap-1 rounded-sm p-2 text-sm transition-all outline-none focus-visible:ring-[3px] focus-visible:outline-1 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )

      div { className = cn("bg-border relative top-[60%] h-2 w-2 rotate-45 rounded-tl-sm shadow-md") }
   }
}
