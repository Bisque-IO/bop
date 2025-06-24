package bop.ui

import lib.cmdk.CommandEmpty
import lib.cmdk.CommandEmptyProps
import lib.cmdk.CommandGroup
import lib.cmdk.CommandGroupProps
import lib.cmdk.CommandInputProps
import lib.cmdk.CommandItem
import lib.cmdk.CommandItemProps
import lib.cmdk.CommandList
import lib.cmdk.CommandListProps
import lib.cmdk.CommandProps
import lib.cmdk.CommandSeparator
import lib.cmdk.CommandSeparatorProps
import lib.lucide.SearchIcon
import react.FC
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.span

val Command = FC<CommandProps>("Command") { props ->
   lib.cmdk.Command {
      +props
      dataSlot = "command"
      className = cn(
         "bg-popover text-popover-foreground flex h-full w-full flex-col overflow-hidden rounded-md", props.className
      )
   }
}

external interface CommandDialogProps : lib.cmdk.CommandDialogProps {
   var title: String?
   var description: String?
}

val CommandDialog = FC<CommandDialogProps>("CommandDialog") { props ->
   Dialog {
      +props
      children = null

      DialogHeader {
         className = cn("sr-only")
         DialogTitle { +(props.title ?: "Command Palette") }
         DialogDescription { +(props.description ?: "Search for a command to run...") }
      }
      DialogContent {
         className = cn("overflow-hidden p-0")
         Command {
            className =
               cn("[&_[cmdk-group-heading]]:text-muted-foreground **:data-[slot=command-input-wrapper]:h-12 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group]]:px-2 [&_[cmdk-group]:not([hidden])_~[cmdk-group]]:pt-0 [&_[cmdk-input-wrapper]_svg]:h-5 [&_[cmdk-input-wrapper]_svg]:w-5 [&_[cmdk-input]]:h-12 [&_[cmdk-item]]:px-2 [&_[cmdk-item]]:py-3 [&_[cmdk-item]_svg]:h-5 [&_[cmdk-item]_svg]:w-5")
            +props.children
         }
      }
   }
}

val CommandInput = FC<CommandInputProps>("CommandInput") { props ->
   div {
      dataSlot = "command-input-wrapper"
      className = cn("flex h-9 items-center gap-2 border-b px-3")

      SearchIcon { className = cn("size-4 shrink-0 opacity-50") }
      lib.cmdk.CommandInput {
         +props
         dataSlot = "command-input"
         className = cn("placeholder:text-muted-foreground flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-hidden disabled:cursor-not-allowed disabled:opacity-50", props.className)
      }
   }
}

val CommandList = FC<CommandListProps>("CommandList") { props ->
   CommandList {
      +props
      dataSlot = "command-list"
      className = cn("max-h-[300px] scroll-py-1 overflow-x-hidden overflow-y-auto", props.className)
   }
}

val CommandEmpty = FC<CommandEmptyProps>("CommandEmpty") { props ->
   CommandEmpty {
      +props
      dataSlot = "command-empty"
      className = cn("py-6 text-center text-sm", props.className)
   }
}

val CommandGroup = FC<CommandGroupProps>("CommandGroup") { props ->
   CommandGroup {
      +props
      dataSlot = "command-group"
      className = cn(
         "text-foreground [&_[cmdk-group-heading]]:text-muted-foreground overflow-hidden p-1 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium",
         props.className
      )
   }
}

val CommandSeparator = FC<CommandSeparatorProps>("CommandSeparator") { props ->
   CommandSeparator {
      +props
      dataSlot = "command-separator"
      className = cn("bg-border -mx-1 h-px", props.className)
   }
}

val CommandItem = FC<CommandItemProps>("CommandItem") { props ->
   CommandItem {
      +props
      dataSlot = "command-item"
      className = cn(
         "data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
         props.className
      )
   }
}

val CommandShortcut = FC<DefaultProps>("CommandShortcut") { props ->
   span {
      +props
      dataSlot = "command-shortcut"
      className = cn("text-muted-foreground ml-auto text-xs tracking-widest", props.className)
   }
}
