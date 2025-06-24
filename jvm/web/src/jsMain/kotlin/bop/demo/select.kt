package bop.demo

import bop.ui.*
import js.objects.unsafeJso
import lib.lucide.ChartBarIcon
import lib.lucide.ChartLineIcon
import lib.lucide.ChartPieIcon
import lib.radix.CircleBackslashIcon
import react.FC
import react.Fragment
import react.createElement
import react.dom.html.ReactHTML.div

external interface TextProps : DefaultProps {
   var text: String
}

val Text = FC<TextProps> { props ->
   +props.text
}

val PlaceholderWithIcon = FC<TextProps> { props ->
   Fragment {
      CircleBackslashIcon {
         className = cn("text-muted-foreground")
      }
      +props.text
   }
}

val SelectDemo = FC {
   div {
      Select {
         SelectTrigger {
            className = cn("w-[180px]")
            SelectValue {
               placeholder = createElement(Text, unsafeJso { text = "Select a fruit" })
            }
         }
         SelectContent {
            SelectGroup {
               SelectLabel { +"Fruits" }
               SelectItem {
                  value = "apple"
                  +"Apple"
               }
               SelectItem {
                  value = "banana"
                  +"Banana"
               }
               SelectItem {
                  value = "blueberry"
                  +"Blueberry"
               }
               SelectItem {
                  value = "grapes"
                  disabled = true
                  +"Grapes"
               }
               SelectItem {
                  value = "pineapple"
                  +"Pineapple"
               }
            }
         }
      }

      Select {
         SelectTrigger {
            className = cn("w-[180px]")
            SelectValue {
               placeholder = createElement(Text, unsafeJso { text = "Large List" })
            }
         }
         SelectContent {
            repeat(100) {
               SelectItem {
                  key = it.toString()
                  value = "item-$it"
                  +"Item $it"
               }
            }
         }
      }

      Select {
         disabled = true
         SelectTrigger {
            className = cn("w-[180px]")
            SelectValue {
               placeholder = createElement(Text, unsafeJso { text = "Disabled" })
            }
         }
         SelectContent {
            SelectGroup {
               SelectLabel { +"Fruits" }
               SelectItem {
                  value = "apple"
                  +"Apple"
               }
               SelectItem {
                  value = "banana"
                  +"Banana"
               }
               SelectItem {
                  value = "blueberry"
                  +"Blueberry"
               }
               SelectItem {
                  value = "grapes"
                  disabled = true
                  +"Grapes"
               }
               SelectItem {
                  value = "pineapple"
                  +"Pineapple"
               }
            }
         }
      }

      Select {
         SelectTrigger {
            className = cn("w-[180px]")
            SelectValue {
               placeholder = createElement(PlaceholderWithIcon, unsafeJso { text = "With Icon" })
            }
         }
         SelectContent {
            SelectItem {
               value = "line"
               ChartLineIcon {}
               +"Line"
            }
            SelectItem {
               value = "bar"
               ChartBarIcon {}
               +"Bar"
            }
            SelectItem {
               value = "pie"
               ChartPieIcon {}
               +"Pie"
            }
         }
      }
   }
}