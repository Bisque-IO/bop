package bop.ui


import cmdk.CommandEmptyProps
import cmdk.CommandGroupProps
import cmdk.CommandItemProps
import cmdk.CommandListProps
import cmdk.CommandProps
import cmdk.CommandSeparatorProps
import react.FC
import react.dom.html.ReactHTML.span
import web.cssom.ClassName

val Command = FC<CommandProps>("Collapsible") { props ->
   cmdk.Command {
      dataSlot = "command"
      className = cn("bg-popover text-popover-foreground flex h-full w-full flex-col overflow-hidden rounded-md", props.className)
      spread(props, "className")
   }
}

external interface CommandDialogProps : cmdk.CommandDialogProps {
   var title: String?
   var description: String?
}

val CommandDialog = FC<CommandDialogProps>("CommandDialog") { props ->
   Dialog {
      spread(props, "children")

      DialogHeader {
         DialogTitle { +(props.title ?: "Command Palette") }
         DialogDescription { +(props.description ?: "Search for a command to run...") }
      }
      DialogContent {
         Command {
            className = ClassName("[&_[cmdk-group-heading]]:text-muted-foreground **:data-[slot=command-input-wrapper]:h-12 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group]]:px-2 [&_[cmdk-group]:not([hidden])_~[cmdk-group]]:pt-0 [&_[cmdk-input-wrapper]_svg]:h-5 [&_[cmdk-input-wrapper]_svg]:w-5 [&_[cmdk-input]]:h-12 [&_[cmdk-item]]:px-2 [&_[cmdk-item]]:py-3 [&_[cmdk-item]_svg]:h-5 [&_[cmdk-item]_svg]:w-5")
            +props.children
         }
      }
   }
}

val CommandList = FC<CommandListProps>("CommandList") { props ->
   cmdk.CommandList {
      dataSlot = "command-list"
      className = cn("max-h-[300px] scroll-py-1 overflow-x-hidden overflow-y-auto", props.className)
      spread(props, "className")
   }
}

val CommandEmpty = FC<CommandEmptyProps>("CommandEmpty") { props ->
   cmdk.CommandEmpty {
      dataSlot = "command-empty"
      className = cn("py-6 text-center text-sm", props.className)
      spread(props, "className")
   }
}

val CommandGroup = FC<CommandGroupProps>("CommandGroup") { props ->
   cmdk.CommandGroup {
      dataSlot = "command-group"
      className = cn("text-foreground [&_[cmdk-group-heading]]:text-muted-foreground overflow-hidden p-1 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium", props.className)
      spread(props, "className")
   }
}

val CommandSeparator = FC<CommandSeparatorProps>("CommandSeparator") { props ->
   cmdk.CommandSeparator {
      dataSlot = "command-separator"
      className = cn("bg-border -mx-1 h-px", props.className)
      spread(props, "className")
   }
}

val CommandItem = FC<CommandItemProps>("CommandItem") { props ->
   cmdk.CommandItem {
      dataSlot = "command-item"
      className = cn("data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground [&_svg:not([class*='text-'])]:text-muted-foreground relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden select-none data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4", props.className)
      spread(props, "className")
   }
}

val CommandShortcut = FC<DefaultProps>("CommandShortcut") { props ->
   span {
      dataSlot = "command-shortcut"
      className = cn("text-muted-foreground ml-auto text-xs tracking-widest", props.className)
      spread(props, "className")
   }
}
