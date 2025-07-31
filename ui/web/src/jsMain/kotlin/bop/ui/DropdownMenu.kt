package bop.ui

import lib.lucide.CheckIcon
import lib.lucide.ChevronRightIcon
import lib.lucide.CircleIcon
import lib.radix.*
import react.FC
import react.dom.html.ReactHTML.span
import web.cssom.ClassName

val DropdownMenu = FC<DropdownMenuRootProps>("DropdownMenu") { props ->
   DropdownMenuPrimitiveRoot {
      +props
      dataSlot = "dropdown-menu"
   }
}

val DropdownMenuPortal = FC<DropdownMenuPortalProps>("DropdownMenuPortal") { props ->
   DropdownMenuPrimitivePortal {
      +props
      dataSlot = "dropdown-menu-portal"
   }
}

val DropdownMenuTrigger = FC<DropdownMenuTriggerProps>("DropdownMenuTrigger") { props ->
   DropdownMenuPrimitiveTrigger {
      +props
      dataSlot = "dropdown-menu-trigger"
   }
}

val DropdownMenuContent = FC<DropdownMenuContentProps>("DropdownMenuContent") { props ->
   DropdownMenuPrimitivePortal {
      DropdownMenuPrimitiveContent {
         +props
         dataSlot = "dropdown-menu-content"
         className = cn(
            "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 max-h-(--radix-dropdown-menu-content-available-height) min-w-[8rem] origin-(--radix-dropdown-menu-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-md border p-1 shadow-md",
            props.className
         )
         sideOffset = props.sideOffset ?: 4
      }
   }
}

val DropdownMenuGroup = FC<DropdownMenuGroupProps>("DropdownMenuGroup") { props ->
   DropdownMenuPrimitiveGroup {
      +props
      dataSlot = "dropdown-menu-group"
   }
}

external interface DropdownMenuItemProps : lib.radix.DropdownMenuItemProps {
   var inset: Boolean?
   var variant: String? // "default" | "destructive"
}

val DropdownMenuItem = FC<DropdownMenuItemProps>("DropdownMenuItem") { props ->
   DropdownMenuPrimitiveItem {
      +props
      dataSlot = "dropdown-menu-item"
      dataInset = props.inset
      dataVariant = props.variant ?: "default"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:!text-destructive [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
   }
}

val DropdownMenuCheckboxItem = FC<DropdownMenuCheckboxItemProps>("DropdownMenuCheckboxItem") { props ->
   DropdownMenuPrimitiveCheckboxItem {
      +props
      children = null
      dataSlot = "dropdown-menu-checkbox-item"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
      checked = props.checked

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")

         DropdownMenuPrimitiveItemIndicator {
            CheckIcon {
               className = ClassName("size-4")
            }
         }
      }

      +props.children
   }
}

val DropdownMenuRadioGroup = FC<DropdownMenuRadioGroupProps>("DropdownMenuRadioGroup") { props ->
   DropdownMenuPrimitiveRadioGroup {
      +props
      dataSlot = "dropdown-menu-radio-group"
   }
}

val DropdownMenuRadioItem = FC<DropdownMenuRadioItemProps>("DropdownMenuRadioItem") { props ->
   DropdownMenuPrimitiveCheckboxItem {
      +props
      children = null
      dataSlot = "dropdown-menu-radio-item"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")

         DropdownMenuPrimitiveItemIndicator {
            CircleIcon {
               className = ClassName("size-2 fill-current")
            }
         }
      }

      +props.children
   }
}

external interface DropdownMenuLabelProps : lib.radix.DropdownMenuLabelProps {
   var inset: dynamic
}

val DropdownMenuLabel = FC<DropdownMenuLabelProps>("DropdownMenuLabel") { props ->
   DropdownMenuPrimitiveLabel {
      +props
      dataSlot = "dropdown-menu-label"
      dataInset = props.inset
      className = cn("px-2 py-1.5 text-sm font-medium data-[inset]:pl-8", props.className)
   }
}

val DropdownMenuSeparator = FC<DropdownMenuSeparatorProps>("DropdownMenuSeparator") { props ->
   DropdownMenuPrimitiveSeparator {
      +props
      dataSlot = "dropdown-menu-separator"
      className = cn("bg-border -mx-1 my-1 h-px", props.className)
   }
}

val DropdownMenuShortcut = FC<DefaultProps>("DropdownMenuShortcut") { props ->
   span {
      +props
      dataSlot = "dropdown-menu-shortcut"
      className = cn("text-muted-foreground ml-auto text-xs tracking-widest", props.className)
   }
}

val DropdownMenuSub = FC<DropdownMenuSubProps>("DropdownMenuSub") { props ->
   DropdownMenuPrimitiveSub {
      +props
      dataSlot = "dropdown-menu-sub"
   }
}

val DropdownMenuSubTrigger = FC<DropdownMenuSubTriggerProps>("DropdownMenuSubTrigger") { props ->
   DropdownMenuPrimitiveSubTrigger {
      +props
      children = null
      dataSlot = "dropdown-menu-sub-trigger"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground flex cursor-default items-center rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[inset]:pl-8",
         props.className
      )

      +props.children
      ChevronRightIcon {
         className = ClassName("ml-auto size-4")
      }
   }
}

val DropdownMenuSubContent = FC<DropdownMenuSubContentProps>("DropdownMenuSubContent") { props ->
   DropdownMenuPrimitiveSubContent {
      +props
      dataSlot = "dropdown-menu-sub-content"
      className = cn(
         "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 min-w-[8rem] origin-(--radix-dropdown-menu-content-transform-origin) overflow-hidden rounded-md border p-1 shadow-lg",
         props.className
      )
   }
}
