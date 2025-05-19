package bop.demo

import bop.ui.Label
import bop.ui.RadioGroup
import bop.ui.RadioGroupItem
import bop.ui.cn
import js.objects.unsafeJso
import react.FC
import react.dom.html.ReactHTML.div

external interface Plan {
   var id: String
   var name: String
   var description: String
   var price: String
}

val plans: Array<Plan> = arrayOf(unsafeJso {
   id = "starter"
   name = "Starter Plan"
   description = "Perfect for small businesses getting started with our platform"
   price = "$10"
}, unsafeJso {
   id = "pro"
   name = "Pro Plan"
   description = "Advanced features for growing businesses with higher demands"
   price = "$20"
})

val RadioGroupDemo = FC {
   div {
      className = cn("flex flex-col gap-6")

      RadioGroup {
         defaultValue = "comfortable"
         div {
            className = cn("flex items-center gap-3")
            RadioGroupItem {
               value = "default"
               id = "r1"
            }
            Label {
               htmlFor = "r1"
               +"Default"
            }
         }
         div {
            className = cn("flex items-center gap-3")
            RadioGroupItem {
               value = "comfortable"
               id = "r2"
            }
            Label {
               htmlFor = "r2"
               +"Comfortable"
            }
         }
         div {
            className = cn("flex items-center gap-3")
            RadioGroupItem {
               value = "compact"
               id = "r3"
            }
            Label {
               htmlFor = "r3"
               +"Compact"
            }
         }
      }

      RadioGroup {
         defaultValue = "starter"
         className = cn("max-w-sm")

         plans.forEach { plan ->
            Label {
               className =
                  cn("hover:bg-accent/50 flex items-start gap-3 rounded-lg border p-4 has-[[data-state=checked]]:border-green-600 has-[[data-state=checked]]:bg-green-50 dark:has-[[data-state=checked]]:border-green-900 dark:has-[[data-state=checked]]:bg-green-950")
               key = plan.id

               RadioGroupItem {
                  value = plan.id
                  id = plan.name
                  className =
                     cn("shadow-none data-[state=checked]:border-green-600 data-[state=checked]:bg-green-600 *:data-[slot=radio-group-indicator]:[&>svg]:fill-white *:data-[slot=radio-group-indicator]:[&>svg]:stroke-white")
               }
               div {
                  className = cn("grid gap-1 font-normal")

                  div {
                     className = cn("font-medium")
                     +plan.name
                  }
                  div {
                     className = cn("text-muted-foreground leading-snug")
                     +plan.description
                  }
               }
            }
         }
      }
   }
}