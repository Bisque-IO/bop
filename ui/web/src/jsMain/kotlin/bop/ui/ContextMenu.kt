package bop.ui

import lib.lucide.CheckIcon
import lib.lucide.ChevronRightIcon
import lib.lucide.CircleIcon
import lib.radix.*
import lib.radix.ContextMenuItemProps
import react.FC
import react.dom.html.ReactHTML.span
import web.cssom.ClassName
import web.cssom.Padding
import web.cssom.px

val ContextMenu = FC<ContextMenuRootProps>("ContextMenu") { props ->
   ContextMenuPrimitiveRoot {
      +props
      dataSlot = "context-menu"
   }
}

val ContextMenuTrigger = FC<ContextMenuTriggerProps>("ContextMenuTrigger") { props ->
   ContextMenuPrimitiveTrigger {
      +props
      dataSlot = "context-menu-trigger"
   }
}

val ContextMenuGroup = FC<ContextMenuGroupProps>("ContextMenuGroup") { props ->
   ContextMenuPrimitiveGroup {
      +props
      dataSlot = "context-menu-group"
   }
}

val ContextMenuPortal = FC<ContextMenuPortalProps>("ContextMenuPortal") { props ->
   ContextMenuPrimitivePortal {
      +props
      dataSlot = "context-menu-portal"
   }
}

val ContextMenuSub = FC<ContextMenuSubProps>("ContextMenuSub") { props ->
   ContextMenuPrimitiveSub {
      +props
      dataSlot = "context-menu-sub"
   }
}

val ContextMenuRadioGroup = FC<ContextMenuRadioGroupProps>("ContextMenuRadioGroup") { props ->
   ContextMenuPrimitiveRadioGroup {
      +props
      dataSlot = "context-menu-radio-group"
   }
}

external interface ContextMenuSubTriggerProps : lib.radix.ContextMenuSubTriggerProps {
   var inset: Boolean?
}

val ContextMenuSubTrigger = FC<ContextMenuSubTriggerProps>("ContextMenuSubTrigger") { props ->
   ContextMenuPrimitiveSubTrigger {
      +props
      children = null
      dataSlot = "context-menu-sub-trigger"
      dataInset = props.inset
      className = cn(
         "focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground flex cursor-default items-center rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
      +props.children
      ChevronRightIcon {
         className = ClassName("ml-auto")
      }
   }
}

val ContextMenuSubContent = FC<ContextMenuSubContentProps>("ContextMenuSubContent") { props ->
   ContextMenuPrimitiveSubContent {
      +props
      dataSlot = "context-menu-sub-content"
      className = cn(
         "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 min-w-[8rem] origin-(--radix-context-menu-content-transform-origin) overflow-hidden rounded-md border p-1 shadow-lg",
         props.className
      )
   }
}

val ContextMenuContent = FC<ContextMenuContentProps>("ContextMenuContent") { props ->
   ContextMenuPrimitivePortal {
      ContextMenuPrimitiveContent {
         +props
         collisionPadding = CollisionPadding(20)
         dataSlot = "context-menu-content"
         className = cn(
            "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 max-h-(--radix-context-menu-content-available-height) min-w-[8rem] origin-(--radix-context-menu-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-md border p-1 shadow-md",
            props.className
         )
      }
   }
}

external interface ContextMenuItemProps : ContextMenuItemProps {
   var inset: Boolean?
   var variant: String? // "default" | "destructive"
}

val ContextMenuItem = FC<ContextMenuItemProps>("ContextMenuItem") { props ->
   ContextMenuPrimitiveItem {
      +props
      dataSlot = "context-menu-item"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground data-[variant=destructive]:text-destructive data-[variant=destructive]:focus:bg-destructive/10 dark:data-[variant=destructive]:focus:bg-destructive/20 data-[variant=destructive]:focus:text-destructive data-[variant=destructive]:*:[svg]:!text-destructive [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[inset]:pl-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
   }
}

val ContextMenuCheckboxItem = FC<ContextMenuCheckboxItemProps>("ContextMenuCheckboxItem") { props ->
   ContextMenuPrimitiveCheckboxItem {
      +props
      children = null
      dataSlot = "context-menu-checkbox-item"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
      checked = props.checked

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")
         ContextMenuPrimitiveItemIndicator {
            CheckIcon {
               className = ClassName("size-4")
            }
         }
      }

      +props.children
   }
}

val ContextMenuRadioItem = FC<ContextMenuRadioItemProps>("ContextMenuRadioItem") { props ->
   ContextMenuPrimitiveRadioItem {
      +props
      children = null
      dataSlot = "context-menu-radio-item"
      className = cn(
         "focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )

      span {
         className = ClassName("pointer-events-none absolute left-2 flex size-3.5 items-center justify-center")
         ContextMenuPrimitiveItemIndicator {
            CircleIcon {
               className = ClassName("size-2 fill-current")
            }
         }
      }

      +props.children
   }
}

external interface ContextMenuLabelProps : lib.radix.ContextMenuLabelProps {
   var inset: Boolean?
}

val ContextMenuLabel = FC<ContextMenuLabelProps>("ContextMenuLabel") { props ->
   ContextMenuPrimitiveLabel {
      +props
      dataSlot = "context-menu-label"
      dataInset = props.inset
      className = cn("text-foreground px-2 py-1.5 text-sm font-medium data-[inset]:pl-8", props.className)
   }
}

val ContextMenuSeparator = FC<ContextMenuSeparatorProps>("ContextMenuSeparator") { props ->
   ContextMenuPrimitiveSeparator {
      +props
      dataSlot = "context-menu-separator"
      className = cn("bg-border -mx-1 my-1 h-px", props.className)
   }
}

val ContextMenuShortcut = FC<DefaultProps>("ContextMenuShortcut") { props ->
   ContextMenuPrimitiveSeparator {
      +props
      dataSlot = "context-menu-shortcut"
      className = cn("text-muted-foreground ml-auto text-xs tracking-widest", props.className)
   }
}