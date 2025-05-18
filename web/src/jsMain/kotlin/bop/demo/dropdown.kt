package bop.demo

import bop.ui.Button
import bop.ui.DropdownMenu
import bop.ui.DropdownMenuContent
import bop.ui.DropdownMenuGroup
import bop.ui.DropdownMenuItem
import bop.ui.DropdownMenuLabel
import bop.ui.DropdownMenuSeparator
import bop.ui.DropdownMenuShortcut
import bop.ui.DropdownMenuTrigger
import react.FC
import web.cssom.ClassName

val DropdownMenuSimple = FC {
   DropdownMenu {
      defaultOpen = false
      DropdownMenuTrigger {
         asChild = true

         Button {
            variant = "outline"
            +"Open"
         }
      }

      DropdownMenuContent {
         align = "start"
         className = ClassName("w-56")

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