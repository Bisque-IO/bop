package bop.ui

import lucide.CheckIcon
import radix.ui.ChevronRightIcon
import radix.ui.CircleIcon
import radix.ui.ContextMenuCheckboxItemProps
import radix.ui.ContextMenuContentProps
import radix.ui.ContextMenuGroupProps
import radix.ui.ContextMenuItemProps
import radix.ui.ContextMenuLabelProps
import radix.ui.ContextMenuPortalProps
import radix.ui.ContextMenuRadioGroupProps
import radix.ui.ContextMenuRadioItemProps
import radix.ui.ContextMenuRootProps
import radix.ui.ContextMenuSeparatorProps
import radix.ui.ContextMenuSubContentProps
import radix.ui.ContextMenuSubProps
import radix.ui.ContextMenuSubTriggerProps
import radix.ui.ContextMenuTriggerProps
import react.FC
import react.dom.html.ReactHTML.span
import web.cssom.ClassName

val ContextMenu = FC<ContextMenuRootProps>("ContextMenu") { props ->
   radix.ui.ContextMenuRoot {
      dataSlot = "context-menu"
      spread(props)
   }
}

val ContextMenuTrigger = FC<ContextMenuTriggerProps>("ContextMenuTrigger") { props ->
   radix.ui.ContextMenuTrigger {
      dataSlot = "context-menu-trigger"
      spread(props)
   }
}

val ContextMenuGroup = FC<ContextMenuGroupProps>("ContextMenuGroup") { props ->
   radix.ui.ContextMenuGroup {
      dataSlot = "context-menu-group"
      spread(props)
   }
}

val ContextMenuPortal = FC<ContextMenuPortalProps>("ContextMenuPortal") { props ->
   radix.ui.ContextMenuPortal {
      dataSlot = "context-menu-portal"
      spread(props)
   }
}

val ContextMenuSub = FC<ContextMenuSubProps>("ContextMenuSub") { props ->
   radix.ui.ContextMenuSub {
      dataSlot = "context-menu-sub"
      spread(props)
   }
}

val ContextMenuRadioGroup = FC<ContextMenuRadioGroupProps>("ContextMenuRadioGroup") { props ->
   radix.ui.ContextMenuRadioGroup {
      dataSlot = "context-menu-radio-group"
      spread(props)
   }
}

val ContextMenuSubTrigger = FC<ContextMenuSubTriggerProps>("ContextMenuSubTrigger") { props ->
   radix.ui.ContextMenuSubTrigger {
      dataSlot = "context-menu-sub-trigger"
      this["data-inset"] = props.inset
      className = cn("focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground flex cursor-default items-center rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)
      spread(props, "className", "children")
      +props.children
      ChevronRightIcon {
         className = ClassName("ml-auto")
      }
   }
}

val ContextMenuSubContent = FC<ContextMenuSubContentProps>("ContextMenuSubContent") { props ->
   radix.ui.ContextMenuSubContent {
      dataSlot = "context-menu-sub-content"
      className = cn("bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 min-w-[8rem] origin-(--radix-context-menu-content-transform-origin) overflow-hidden rounded-md border p-1 shadow-lg", props.className)
      spread(props, "className")
   }
}

val ContextMenuContent = FC<ContextMenuContentProps>("ContextMenuContent") { props ->
   radix.ui.ContextMenuPortal {
      radix.ui.ContextMenuContent {
         dataSlot = "context-menu-content"
         className = cn("bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 max-h-(--radix-context-menu-content-available-height) min-w-[8rem] origin-(--radix-context-menu-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-md border p-1 shadow-md", props.className)
         spread(props, "className")
      }
   }
}

external interface ContextMenuItemProps : radix.ui.ContextMenuItemProps {
   var inset: Boolean?
   var variant: String? // "default" | "destructive"
}

val ContextMenuItem = FC<ContextMenuItemProps>("ContextMenuItem") { props ->
   radix.ui.ContextMenuItem {
      dataSlot = "context-menu-item"
      className = cn("focus:bg-accent focus:text-accent-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:!text-destructive [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)
      spread(props, "className")
   }
}

val ContextMenuCheckboxItem = FC<ContextMenuCheckboxItemProps>("ContextMenuCheckboxItem") { props ->
   radix.ui.ContextMenuCheckboxItem {
      dataSlot = "context-menu-checkbox-item"
      className = cn("focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)
      spread(props, "className", "children")
      checked = props.checked

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")
         radix.ui.ContextMenuItemIndicator {
            CheckIcon {
               className = "size-4"
            }
         }
      }

      +props.children
   }
}

val ContextMenuRadioItem = FC<ContextMenuRadioItemProps>("ContextMenuRadioItem") { props ->
   radix.ui.ContextMenuRadioItem {
      dataSlot = "context-menu-radio-item"
      className = cn("focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)
      spread(props, "className", "children")

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")
         radix.ui.ContextMenuItemIndicator {
            CircleIcon {
               className = ClassName("size-2 fill-current")
            }
         }
      }

      +props.children
   }
}

val ContextMenuLabel = FC<ContextMenuLabelProps>("ContextMenuLabel") { props ->
   radix.ui.ContextMenuLabel {
      dataSlot = "context-menu-label"
      this["data-inset"] = props.inset
      className = cn("text-foreground px-2 py-1.5 text-sm font-medium data-[inset]:pl-8", props.className)
      spread(props, "className")
   }
}

val ContextMenuSeparator = FC<ContextMenuSeparatorProps>("ContextMenuSeparator") { props ->
   radix.ui.ContextMenuSeparator {
      dataSlot = "context-menu-separator"
      className = cn("bg-border -mx-1 my-1 h-px", props.className)
      spread(props, "className")
   }
}

val ContextMenuShortcut = FC<DefaultProps>("ContextMenuShortcut") { props ->
   radix.ui.ContextMenuSeparator {
      dataSlot = "context-menu-shortcut"
      className = cn("text-muted-foreground ml-auto text-xs tracking-widest", props.className)
      spread(props, "className")
   }
}