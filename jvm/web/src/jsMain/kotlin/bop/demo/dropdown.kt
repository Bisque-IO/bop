package bop.demo

import bop.ui.*
import react.FC

val DropdownMenuSimple = FC {
   DropdownMenu {
      DropdownMenuTrigger {
         asChild = true

         Button {
            variant = "secondary"
            +"Open"
         }
      }
      DropdownMenuContent {
         align = "start"
         className = cn("w-56")

         DropdownMenuLabel {
            +"MyAccount"
         }
         DropdownMenuSeparator {}
         DropdownMenuGroup {
            DropdownMenuItem {
               +"Profile"
               DropdownMenuShortcut {
                  +"⇧⌘P"
               }
            }
            DropdownMenuItem {
               +"Billing"
               DropdownMenuShortcut {
                  +"⌘B"
               }
            }
            DropdownMenuItem {
               +"Settings"
               DropdownMenuShortcut {
                  +"⌘S"
               }
            }
            DropdownMenuItem {
               +"Keyboard shortcuts"
               DropdownMenuShortcut {
                  +"⌘K"
               }
            }
         }
      }
   }
}
