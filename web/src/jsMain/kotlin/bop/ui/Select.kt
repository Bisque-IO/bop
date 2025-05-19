package bop.ui

import js.reflect.unsafeCast
import lib.lucide.CheckIcon
import lib.lucide.ChevronDownIcon
import lib.lucide.ChevronUpIcon
import lib.radix.SelectContentProps
import lib.radix.SelectGroupProps
import lib.radix.SelectItemProps
import lib.radix.SelectLabelProps
import lib.radix.SelectPosition
import lib.radix.SelectPrimitiveContent
import lib.radix.SelectPrimitiveGroup
import lib.radix.SelectPrimitiveIcon
import lib.radix.SelectPrimitiveItem
import lib.radix.SelectPrimitiveItemIndicator
import lib.radix.SelectPrimitiveItemText
import lib.radix.SelectPrimitiveLabel
import lib.radix.SelectPrimitivePortal
import lib.radix.SelectPrimitiveRoot
import lib.radix.SelectPrimitiveScrollUpButton
import lib.radix.SelectPrimitiveSeparator
import lib.radix.SelectPrimitiveTrigger
import lib.radix.SelectPrimitiveValue
import lib.radix.SelectPrimitiveViewport
import lib.radix.SelectRootProps
import lib.radix.SelectScrollDownButtonProps
import lib.radix.SelectScrollUpButtonProps
import lib.radix.SelectSeparatorProps
import lib.radix.SelectValueProps
import react.FC
import react.dom.html.ReactHTML.span

val Select = FC<SelectRootProps>("Select") { props ->
   SelectPrimitiveRoot {
      +props
      dataSlot = "select"
   }
}

val SelectGroup = FC<SelectGroupProps>("SelectGroup") { props ->
   SelectPrimitiveGroup {
      +props
      dataSlot = "select-group"
   }
}

val SelectValue = FC<SelectValueProps>("SelectValue") { props ->
   SelectPrimitiveValue {
      +props
      dataSlot = "select-value"
   }
}

sealed interface SelectTriggerSize {
   companion object {
      val sm: SelectTriggerSize = unsafeCast("sm")
      val default: SelectTriggerSize = unsafeCast("default")
   }
}

external interface SelectTriggerProps : lib.radix.SelectTriggerProps {
   var size: SelectTriggerSize?
}

val SelectTrigger = FC<SelectTriggerProps>("SelectTrigger") { props ->
   SelectPrimitiveTrigger {
      +props
      children = null
      dataSlot = "select-trigger"
      dataSize = props.size?.toString() ?: SelectTriggerSize.default.toString()
      className = cn("border-input data-[placeholder]:text-muted-foreground [&_svg:not([class*='text-'])]:text-muted-foreground focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:bg-input/30 dark:hover:bg-input/50 flex w-fit items-center justify-between gap-2 rounded-md border bg-transparent px-3 py-2 text-sm whitespace-nowrap shadow-xs transition-[color,box-shadow] outline-none focus-visible:ring-[3px] disabled:cursor-not-allowed disabled:opacity-50 data-[size=default]:h-9 data-[size=sm]:h-8 *:data-[slot=select-value]:line-clamp-1 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-2 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)

      +props.children
      SelectPrimitiveIcon {
         asChild = true
         ChevronDownIcon {
            className = cn("size-4 opacity-50")
         }
      }
   }
}

val SelectContent = FC<SelectContentProps>("SelectContent") { props ->
   SelectPrimitivePortal {
      SelectPrimitiveContent {
         +props
         children = null
         dataSlot = "select-content"
         className = cn(
            "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 relative z-50 max-h-(--radix-select-content-available-height) min-w-[8rem] origin-(--radix-select-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-md border shadow-md",
            when(props.position) {
               SelectPosition.popper -> "data-[side=bottom]:translate-y-1 data-[side=left]:-translate-x-1 data-[side=right]:translate-x-1 data-[side=top]:-translate-y-1"
               null -> ""
            },
            props.className
         )
         position = props.position

         SelectScrollUpButton {}
         SelectPrimitiveViewport {
            className = cn(
               "p-1",
               when(props.position) {
                  SelectPosition.popper -> "h-[var(--radix-select-trigger-height)] w-full min-w-[var(--radix-select-trigger-width)] scroll-my-1"
                  else -> ""
               }
            )
            +props.children
         }
         SelectScrollDownButton {}
      }
   }
}

val SelectItem = FC<SelectItemProps>("SelectItem") { props ->
   SelectPrimitiveItem {
      +props
      children = null
      dataSlot = "select-item"
      className = cn("focus:bg-accent focus:text-accent-foreground [&_svg:not([class*='text-'])]:text-muted-foreground relative flex w-full cursor-default items-center gap-2 rounded-sm py-1.5 pr-8 pl-2 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 *:[span]:last:flex *:[span]:last:items-center *:[span]:last:gap-2", props.className)

      span {
         className = cn("absolute right-2 flex size-3.5 items-center justify-center")
         SelectPrimitiveItemIndicator {
            CheckIcon {
               className = cn("size-4")
            }
         }
      }

      SelectPrimitiveItemText { +props.children }
   }
}

val SelectLabel = FC<SelectLabelProps>("SelectLabel") { props ->
   SelectPrimitiveLabel {
      +props
      dataSlot = "select-label"
      className = cn("text-muted-foreground px-2 py-1.5 text-xs", props.className)
   }
}

val SelectSeparator = FC<SelectSeparatorProps>("SelectSeparator") { props ->
   SelectPrimitiveSeparator {
      +props
      dataSlot = "select-separator"
      className = cn("bg-border pointer-events-none -mx-1 my-1 h-px", props.className)
   }
}

val SelectScrollUpButton = FC<SelectScrollUpButtonProps>("SelectScrollUpButton") { props ->
   SelectPrimitiveScrollUpButton {
      +props
      dataSlot = "select-scroll-up-button"
      className = cn("flex cursor-default items-center justify-center py-1", props.className)

      ChevronUpIcon {
         className = cn("size-4")
      }
   }
}

val SelectScrollDownButton = FC<SelectScrollDownButtonProps>("SelectScrollDownButton") { props ->
   SelectPrimitiveScrollUpButton {
      +props
      dataSlot = "select-scroll-down-button"
      className = cn("flex cursor-default items-center justify-center py-1", props.className)

      ChevronDownIcon {
         className = cn("size-4")
      }
   }
}