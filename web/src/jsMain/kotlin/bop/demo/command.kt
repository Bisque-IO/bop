package bop.demo

import bop.ui.CommandDialog
import bop.ui.CommandEmpty
import bop.ui.CommandGroup
import bop.ui.CommandInput
import bop.ui.CommandItem
import bop.ui.CommandList
import bop.ui.CommandShortcut
import bop.ui.cn
import lib.lucide.CalculatorIcon
import lib.lucide.CalendarIcon
import lib.lucide.CreditCardIcon
import lib.lucide.SettingsIcon
import lib.lucide.SmileIcon
import lib.lucide.UserIcon
import react.FC
import react.Fragment
import react.dom.html.ReactHTML.kbd
import react.dom.html.ReactHTML.p
import react.dom.html.ReactHTML.span
import react.useEffectWithCleanup
import react.useState
import web.dom.document
import web.events.EventType
import web.events.addEventListener
import web.events.removeEventListener
import web.uievents.KeyboardEvent

val CommandDemo = FC {
   val (open, setOpen) = useState(false)

   useEffectWithCleanup {
      val down = { e: KeyboardEvent ->
         if (e.key == "j" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault()
            setOpen { !it }
         }
      }

      document.addEventListener(EventType("keydown"), down)
      onCleanup {
         document.removeEventListener(
            EventType("keydown"),
            down
         )
      }
   }

   Fragment {
      p {
         className = cn("text-muted-foreground text-sm")
         +"Press "
         kbd {
            className = cn("bg-muted text-muted-foreground pointer-events-none inline-flex h-5 items-center gap-1 rounded border px-1.5 font-mono text-[10px] font-medium opacity-100 select-none")
            span {
               className = cn("text-xs")
               +"⌘"
            }
            +"J"
         }
      }
      CommandDialog {
         this.open = open
         onOpenChange = { setOpen(it) }

         CommandInput {
            placeholder = "Type a command or search..."
         }

         CommandList {
            CommandEmpty { +"No results found." }
            CommandGroup {
               heading = "Suggestions"

               CommandItem {
                  CalendarIcon {}
                  span { +"Calendar" }
               }
               CommandItem {
                  SmileIcon {}
                  span { +"Search Emoji" }
               }
               CommandItem {
                  CalculatorIcon {}
                  span { +"Calculator" }
               }
            }
            CommandGroup {
               heading = "Settings"

               CommandItem {
                  UserIcon {}
                  span { +"Profile" }
                  CommandShortcut { +"⌘P" }
               }
               CommandItem {
                  CreditCardIcon {}
                  span { +"Billing" }
                  CommandShortcut { +"⌘B" }
               }
               CommandItem {
                  SettingsIcon {}
                  span { +"Settings" }
                  CommandShortcut { +"⌘S" }
               }
            }
         }
      }
   }
}