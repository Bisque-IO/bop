package bop.demo

import bop.ui.Button
import bop.ui.cn
import lib.lucide.ArrowLeftIcon
import lib.lucide.HomeIcon
import react.FC
import react.dom.html.ReactHTML.div
import react.router.useNavigate
import web.cssom.ClassName

val ButtonDemo = FC {
   val navigate = useNavigate()

   div {
      className = ClassName("pt-6")

      Button {
         className = cn("mr-4")
         onClick = { navigate("/about") }
         +"Default"
      }
      Button {
         variant = "outline"
         className = ClassName("mr-4")
         onClick = { navigate("/about") }
         +"Outline"
      }
      Button {
         variant = "ghost"
         className = ClassName("mr-4")
         onClick = { navigate("/about") }
         +"Ghost"
      }
      Button {
         variant = "destructive"
         className = ClassName("mr-4")
         onClick = { navigate("/about") }
         +"Destructive"
      }
      Button {
         variant = "secondary"
         className = ClassName("mr-4")
         onClick = { navigate("/about") }
         +"Secondary"
      }
      Button {
         variant = "link"
         className = ClassName("mr-4")
         className = ClassName("m-6")
         onClick = { navigate("/about") }
         +"Link"
      }
      Button {
         variant = "outline"
         className = ClassName("mr-4")
         onClick = { navigate("/about") }
         HomeIcon {}
         +"Home"
      }

      Button {
         variant = "outline"
         size = "lg"
         onClick = { navigate("/about") }
         ArrowLeftIcon {}
         +"Go Back!"
      }
   }
}