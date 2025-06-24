package bop.demo

import bop.ui.Menubar
import bop.ui.MenubarCheckboxItem
import bop.ui.MenubarContent
import bop.ui.MenubarGroup
import bop.ui.MenubarItem
import bop.ui.MenubarMenu
import bop.ui.MenubarRadioGroup
import bop.ui.MenubarRadioItem
import bop.ui.MenubarSeparator
import bop.ui.MenubarShortcut
import bop.ui.MenubarSub
import bop.ui.MenubarSubContent
import bop.ui.MenubarSubTrigger
import bop.ui.MenubarTrigger
import lib.lucide.HelpCircleIcon
import lib.lucide.SettingsIcon
import lib.lucide.TrashIcon
import react.FC

val MenubarDemo = FC {
   Menubar {
      MenubarMenu {
         MenubarTrigger {
            +"File"
         }
         MenubarContent {
            MenubarItem {
               +"New Tab"
               MenubarShortcut { +"⌘T" }
            }
            MenubarItem {
               +"New Window"
               MenubarShortcut { +"⌘N" }
            }
            MenubarItem {
               disabled = true
               +"New Incognito Window"
            }
            MenubarSeparator {}
            MenubarSub {
               MenubarSubTrigger {
                  +"Share"
               }
               MenubarSubContent {
                  MenubarItem { +"Email link" }
                  MenubarItem { +"Messages" }
                  MenubarItem { +"Notes" }
               }
            }
            MenubarSeparator {}
            MenubarItem {
               +"Print"
               MenubarShortcut { +"⌘P" }
            }
         }
      }
      MenubarMenu {
         MenubarTrigger {
            +"Edit"
         }
         MenubarContent {
            MenubarItem {
               +"Undo"
               MenubarShortcut { +"⌘Z" }
            }
            MenubarItem {
               +"Redo"
               MenubarShortcut { +"⇧⌘Z" }
            }
            MenubarSeparator {}
            MenubarSub {
               MenubarSubTrigger {
                  +"Find"
               }
               MenubarSubContent {
                  MenubarItem { +"Search the website" }
                  MenubarSeparator {}
                  MenubarItem { +"Find..." }
                  MenubarItem { +"Find Next" }
                  MenubarItem { +"Find Previous" }
               }
            }
            MenubarSeparator {}
            MenubarItem { +"Cut" }
            MenubarItem { +"Copy" }
            MenubarItem { +"Paste" }
         }
      }
      MenubarMenu {
         MenubarTrigger {
            +"View"
         }
         MenubarContent {
            MenubarCheckboxItem { +"Always Show Bookmarks Bar" }
            MenubarCheckboxItem {
               checked = true
               +"Always Show Full URLs"
            }
            MenubarSeparator {}
            MenubarItem {
               inset = true
               +"Reload"
               MenubarShortcut { +"⌘R" }
            }
            MenubarItem {
               disabled = true
               inset = true
               +"Force Reload"
               MenubarShortcut { +"⇧⌘R" }
            }
            MenubarSeparator {}
            MenubarItem {
               inset = true
               +"Toggle Fullscreen"
            }
            MenubarSeparator {}
            MenubarItem {
               inset = true
               +"Hide Sidebar"
            }
         }
      }
      MenubarMenu {
         MenubarTrigger {
            +"Profiles"
         }
         MenubarContent {
            MenubarRadioGroup {
               value = "benoit"
               MenubarRadioItem {
                  value = "andy"
                  +"Andy"
               }
               MenubarRadioItem {
                  value = "benoit"
                  +"Benoit"
               }
               MenubarRadioItem {
                  value = "luis"
                  +"Luis"
               }
            }
            MenubarSeparator {}
            MenubarItem {
               inset = true
               +"Edit..."
            }
            MenubarSeparator {}
            MenubarItem {
               inset = true
               +"Add Profile..."
            }
         }
      }
      MenubarMenu {
         MenubarTrigger {
            +"More"
         }
         MenubarContent {
            MenubarGroup {
               MenubarItem {
                  SettingsIcon {}
                  +"Settings"
               }
               MenubarItem {
                  HelpCircleIcon {}
                  +"Help"
               }
               MenubarSeparator {}
               MenubarItem {
                  variant = "destructive"
                  TrashIcon {}
                  +"Delete"
               }
            }
         }
      }
   }
}