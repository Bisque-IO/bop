package bop.demo

import bop.ui.ScrollArea
import bop.ui.ScrollBar
import bop.ui.cn
import js.objects.unsafeJso
import lib.radix.Separator
import react.FC
import react.Fragment
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.figcaption
import react.dom.html.ReactHTML.figure
import react.dom.html.ReactHTML.h4
import react.dom.html.ReactHTML.img
import react.dom.html.ReactHTML.span
import web.cssom.Width
import web.cssom.atrule.Orientation
import web.cssom.atrule.orientation

val ScrollAreaDemo = FC {
   div {
      className = cn("flex flex-col gap-6")
      ScrollAreaVertical {}
      ScrollAreaHorizontal {}
   }
}

fun buildTags(): List<String> {
   val tags = mutableListOf<String>()
   repeat(50) { tags.add("v1.2.0-beta.$it") }
   return tags
}

val tags = buildTags()

val ScrollAreaVertical = FC {
   div {
      className = cn("flex flex-col gap-6")
      ScrollArea {
         className = cn("h-72 w-48 rounded-md border")
         div {
            className = cn("p-4")
            h4 {
               className = cn("mb-4 text-sm leading-none font-medium")
               +"Tags"
            }
            tags.forEach { tag ->
               Fragment {
                  key = tag
                  div {
                     className = cn("text-sm")
                     +tag
                  }
                  Separator {
                     className = cn("my-2")
                  }
               }
            }
         }
      }
   }
}

external interface Work {
   var artist: String
   var art: String
}

val works: Array<Work> = arrayOf(
   unsafeJso {
      artist = "Ornella Binni"
      art = "https://images.unsplash.com/photo-1465869185982-5a1a7522cbcb?auto=format&fit=crop&w=300&q=80"
   },
   unsafeJso {
      artist = "Tom Byrom"
      art = "https://images.unsplash.com/photo-1548516173-3cabfa4607e9?auto=format&fit=crop&w=300&q=80"
   },
   unsafeJso {
      artist = "Vladimir Malyav"
      art = "https://images.unsplash.com/photo-1494337480532-3725c85fd2ab?auto=format&fit=crop&w=300&q=80"
   }
)

val ScrollAreaHorizontal = FC {
   ScrollArea {
      className = cn("w-full max-w-96 rounded-md border p-4")
      div {
         className = cn("flex gap-4")
         works.forEach {
            figure {
               key = it.artist
               className = cn("shrink-0")

               div {
                  className = cn("overflow-hidden rounded-md")
                  img {
                     src = it.art
                     alt = "Photo by ${it.artist}"
                     className = cn("aspect-[3/4] h-fit w-fit object-cover")
                     width = 300.0
                     height = 400.0
                  }
               }
               figcaption {
                  className = cn("text-muted-foreground pt-2 text-xs")
                  +"Photo by "
                  span {
                     className = cn("text-foreground font-semibold")
                     +it.artist
                  }
               }
            }
         }
      }
      ScrollBar { orientation = "horizontal" }
   }
}
