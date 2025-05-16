package bop.ui

import lucide.CheckIcon
import lucide.ChevronRightIcon
import radix.ui.CircleIcon
import radix.ui.DropdownMenuCheckboxItemProps
import radix.ui.DropdownMenuContentProps
import radix.ui.DropdownMenuGroupProps
import radix.ui.DropdownMenuLabelProps
import radix.ui.DropdownMenuPortalProps
import radix.ui.DropdownMenuRadioGroupProps
import radix.ui.DropdownMenuRadioItemProps
import radix.ui.DropdownMenuRootProps
import radix.ui.DropdownMenuSeparatorProps
import radix.ui.DropdownMenuSubContentProps
import radix.ui.DropdownMenuSubProps
import radix.ui.DropdownMenuSubTriggerProps
import radix.ui.DropdownMenuTriggerProps
import react.FC
import react.dom.html.ReactHTML.span
import web.cssom.ClassName

val DropdownMenu = FC < DropdownMenuRootProps>("DropdownMenu") { props ->
   radix.ui.DropdownMenuRoot {
      dataSlot = "dropdown-menu"
      spread(props)
   }
}

val DropdownMenuPortal = FC <DropdownMenuPortalProps>("DropdownMenuPortal") { props ->
   radix.ui.DropdownMenuPortal {
      dataSlot = "dropdown-menu-portal"
      spread(props)
   }
}

val DropdownMenuTrigger = FC <DropdownMenuTriggerProps>("DropdownMenuTrigger") { props ->
   radix.ui.DropdownMenuTrigger {
      dataSlot = "dropdown-menu-trigger"
      spread(props)
   }
}

val DropdownMenuContent = FC <DropdownMenuContentProps>("DropdownMenuContent") { props ->
   radix.ui.DropdownMenuPortal {
      radix.ui.DropdownMenuContent {
         dataSlot = "dropdown-menu-content"
         spread(props, "className")
         className = cn("bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 max-h-(--radix-dropdown-menu-content-available-height) min-w-[8rem] origin-(--radix-dropdown-menu-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-md border p-1 shadow-md", props.className)
         sideOffset = props.sideOffset ?: 4
      }
   }
}

val DropdownMenuGroup = FC <DropdownMenuGroupProps>("DropdownMenuGroup") { props ->
   radix.ui.DropdownMenuGroup {
      dataSlot = "dropdown-menu-group"
      spread(props)
   }
}

external interface DropdownMenuItemProps : radix.ui.DropdownMenuItemProps {
   var inset: Boolean?
   var variant: String? // "default" | "destructive"
}

val DropdownMenuItem = FC<DropdownMenuItemProps>("DropdownMenuItem") { props ->
   radix.ui.DropdownMenuItem {
      dataSlot = "dropdown-menu-item"
      dataInset = props.inset
      dataVariant = props.variant ?: "default"
      spread(props, "className")
      className = cn("focus:bg-accent focus:text-accent-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:!text-destructive [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)
   }
}

val DropdownMenuCheckboxItem = FC<DropdownMenuCheckboxItemProps>("DropdownMenuCheckboxItem") { props ->
   radix.ui.DropdownMenuCheckboxItem {
      dataSlot = "dropdown-menu-checkbox-item"
      spread(props, "className", "children")
      className = cn("focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)
      checked = props.checked

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")

         radix.ui.DropdownMenuItemIndicator {
            CheckIcon {
               className = "size-4"
            }
         }
      }

      +props.children
   }
}

val DropdownMenuRadioGroup = FC <DropdownMenuRadioGroupProps>("DropdownMenuRadioGroup") { props ->
   radix.ui.DropdownMenuRadioGroup {
      dataSlot = "dropdown-menu-radio-group"
      spread(props)
   }
}

val DropdownMenuRadioItem = FC<DropdownMenuRadioItemProps>("DropdownMenuRadioItem") { props ->
   radix.ui.DropdownMenuCheckboxItem {
      dataSlot = "dropdown-menu-radio-item"
      spread(props, "className", "children")
      className = cn("focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")

         radix.ui.DropdownMenuItemIndicator {
            CircleIcon {
               className = ClassName("size-2 fill-current")
            }
         }
      }

      +props.children
   }
}

val DropdownMenuLabel = FC<DropdownMenuLabelProps>("DropdownMenuLabel") { props ->
   radix.ui.DropdownMenuLabel {
      dataSlot = "dropdown-menu-label"
      dataInset = props.inset
      spread(props, "className")
      className = cn("px-2 py-1.5 text-sm font-medium data-[inset]:pl-8", props.className)
   }
}

val DropdownMenuSeparator = FC<DropdownMenuSeparatorProps>("DropdownMenuSeparator") { props ->
   radix.ui.DropdownMenuSeparator {
      dataSlot = "dropdown-menu-separator"
      spread(props, "className")
      className = cn("bg-border -mx-1 my-1 h-px", props.className)
   }
}

val DropdownMenuShortcut = FC<DefaultProps>("DropdownMenuShortcut") { props ->
   span {
      dataSlot = "dropdown-menu-shortcut"
      spread(props, "className")
      className = cn("text-muted-foreground ml-auto text-xs tracking-widest", props.className)
   }
}

val DropdownMenuSub = FC<DropdownMenuSubProps>("DropdownMenuSub") { props ->
   radix.ui.DropdownMenuSub {
      dataSlot = "dropdown-menu-sub"
      spread(props)
   }
}

val DropdownMenuSubTrigger = FC<DropdownMenuSubTriggerProps>("DropdownMenuSubTrigger") { props ->
   radix.ui.DropdownMenuSubTrigger {
      dataSlot = "dropdown-menu-sub-trigger"
      spread(props, "className", "children")
      className = cn("focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground flex cursor-default items-center rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[inset]:pl-8", props.className)

      +props.children
      ChevronRightIcon {
         className = "ml-auto size-4"
      }
   }
}

val DropdownMenuSubContent = FC<DropdownMenuSubContentProps>("DropdownMenuSubContent") { props ->
   radix.ui.DropdownMenuSubContent {
      dataSlot = "dropdown-menu-sub-content"
      spread(props, "className")
      className = cn("bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 min-w-[8rem] origin-(--radix-dropdown-menu-content-transform-origin) overflow-hidden rounded-md border p-1 shadow-lg", props.className)
   }
}
