package bop.ui

import lib.lucide.CheckIcon
import lib.lucide.ChevronRightIcon
import lib.lucide.CircleIcon
import lib.radix.*
import react.FC
import react.dom.html.ReactHTML.span
import web.cssom.ClassName

val Menubar = FC<MenubarRootProps>("Menubar") { props ->
   MenubarPrimitiveRoot {
      +props
      dataSlot = "menubar"
      className = cn("bg-background flex h-9 items-center gap-1 rounded-md border p-1 shadow-xs", props.className)
   }
}

val MenubarMenu = FC<MenubarMenuProps>("MenubarMenu") { props ->
   MenubarPrimitiveMenu {
      +props
      dataSlot = "menubar-menu"
   }
}

val MenubarGroup = FC<MenubarGroupProps>("MenubarGroupMenu") { props ->
   MenubarPrimitiveGroup {
      +props
      dataSlot = "menubar-group"
   }
}

val MenubarPortal = FC<MenubarPortalProps>("MenubarPortal") { props ->
   MenubarPrimitivePortal {
      +props
      dataSlot = "menubar-portal"
   }
}

val MenubarRadioGroup = FC<MenubarRadioGroupProps>("MenubarRadioGroup") { props ->
   MenubarPrimitiveRadioGroup {
      +props
      dataSlot = "menubar-radio-group"
   }
}

val MenubarTrigger = FC<MenubarTriggerProps>("MenubarTrigger") { props ->
   MenubarPrimitiveTrigger {
      +props
      dataSlot = "menubar-trigger"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground flex items-center rounded-sm px-2 py-1 text-sm font-medium outline-hidden select-none",
         props.className
      )
   }
}

private val CONTENT_EXCLUDE = setOf("className", "align", "alignOffset", "sideOffset")

val MenubarContent = FC<MenubarContentProps>("MenubarContent") { props ->
   MenubarPrimitivePortal {
      MenubarPrimitiveContent {
         spread(props, CONTENT_EXCLUDE)
         dataSlot = "menubar-content"
         align = props.align ?: "start"
         alignOffset = props.alignOffset ?: -4
         sideOffset = props.sideOffset ?: 8
         className = cn(
            "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 min-w-[12rem] origin-(--radix-menubar-content-transform-origin) overflow-hidden rounded-md border p-1 shadow-md",
            props.className
         )
      }
   }
}

external interface MenubarItemProps : lib.radix.MenubarItemProps {
   var inset: Boolean?
   var variant: String? // "default" | "destructive"
}

val MenubarItem = FC<MenubarItemProps>("MenubarItem") { props ->
   MenubarPrimitiveItem {
      +props
      dataSlot = "menubar-content"
      dataInset = props.inset ?: false
      dataVariant = props.variant ?: "default"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:!text-destructive [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
   }
}

val MenubarCheckboxItem = FC<MenubarCheckboxItemProps>("MenubarCheckboxItem") { props ->
   MenubarPrimitiveCheckboxItem {
      +props
      children = null
      dataSlot = "menubar-checkbox-item"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-xs py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
      checked = props.checked

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")
         MenubarPrimitiveItemIndicator {
            CheckIcon {
               className = ClassName("size-4")
            }
         }
      }

      +props.children
   }
}

val MenubarRadioItem = FC<MenubarRadioItemProps>("MenubarRadioItem") { props ->
   MenubarPrimitiveRadioItem {
      +props
      children = null
      dataSlot = "menubar-radio-item"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-xs py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")
         MenubarPrimitiveItemIndicator {
            CircleIcon {
               className = ClassName("size-2 fill-current")
            }
         }
      }

      +props.children
   }
}

external interface MenubarLabelProps : lib.radix.MenubarLabelProps {
   var inset: Boolean?
}

val MenubarLabel = FC<MenubarLabelProps>("MenubarLabel") { props ->
   MenubarPrimitiveLabel {
      +props
      dataSlot = "menubar-label"
      dataInset = props.inset ?: false
      className = cn(
         "px-2 py-1.5 text-sm font-medium data-[inset]:pl-8", props.className
      )
   }
}

val MenubarSeparator = FC<MenubarSeparatorProps>("MenubarSeparator") { props ->
   MenubarPrimitiveSeparator {
      +props
      dataSlot = "menubar-separator"
      className = cn(
         "bg-border -mx-1 my-1 h-px", props.className
      )
   }
}

val MenubarShortcut = FC<DefaultProps>("MenubarShortcut") { props ->
   span {
      +props
      dataSlot = "menubar-shortcut"
      className = cn(
         "text-muted-foreground ml-auto text-xs tracking-widest", props.className
      )
   }
}

val MenubarSub = FC<MenubarSubProps>("MenubarSub") { props ->
   MenubarPrimitiveSub {
      +props
      dataSlot = "menubar-sub"
   }
}

external interface MenubarSubTriggerProps : lib.radix.MenubarSubTriggerProps {
   var inset: Boolean?
}

val MenubarSubTrigger = FC<MenubarSubTriggerProps>("MenubarSubTrigger") { props ->
   MenubarPrimitiveSubTrigger {
      +props
      children = null
      dataSlot = "menubar-sub-trigger"
      dataInset = props.inset ?: false
      className = cn(
         "focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground flex cursor-default items-center rounded-sm px-2 py-1.5 text-sm outline-none select-none data-[inset]:pl-8",
         props.className
      )

      +props.children
      ChevronRightIcon {
         className = ClassName("ml-auto h-4 w-4")
      }
   }
}

val MenubarSubContent = FC<MenubarSubContentProps>("MenubarSubContent") { props ->
   MenubarPrimitiveSubContent {
      +props
      dataSlot = "menubar-sub-content"
      className = cn(
         "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 min-w-[8rem] origin-(--radix-menubar-content-transform-origin) overflow-hidden rounded-md border p-1 shadow-lg",
         props.className
      )
   }
}
