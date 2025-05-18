package bop.ui

import lib.lucide.ChevronRightIcon
import lib.lucide.MoreHorizontalIcon
import lib.radix.Slot
import react.FC
import react.PropsWithClassName
import react.dom.aria.AriaRole
import react.dom.html.ReactHTML.a
import react.dom.html.ReactHTML.li
import react.dom.html.ReactHTML.nav
import react.dom.html.ReactHTML.span
import web.cssom.ClassName

val Breadcrumb = FC<DefaultProps>("Breadcrumb") { props ->
   nav {
      spread(props)
      dataSlot = "breadcrumb"
      ariaLabel = "breadcrumb"
   }
}

val BreadcrumbList = FC<DefaultProps>("BreadcrumbList") { props ->
   li {
      spread(props, "className")
      dataSlot = "breadcrumb-list"
      className = cn(
         "text-muted-foreground flex flex-wrap items-center gap-1.5 text-sm break-words sm:gap-2.5",
         props.className,
      )
   }
}

val BreadcrumbItem = FC<DefaultProps>("BreadcrumbItem") { props ->
   li {
      spread(props, "className")
      dataSlot = "breadcrumb-item"
      className = cn("inline-flex items-center gap-1.5", props.className)
   }
}

val BreadcrumbLink = FC<AsChildProps>("BreadcrumbLink") { props ->
   val component = if (props.asChild == true) Slot else a
   component {
      spread(props, "className")
      dataSlot = "breadcrumb-link"
      className = cn("hover:text-foreground transition-colors", props.className)
   }
}

val BreadcrumbSeparator = FC<AsChildProps>("BreadcrumbSeparator") { props ->
   li {
      spread(props, ExcludeSets.CLASS_NAME_CHILDREN)
      dataSlot = "breadcrumb-separator"
      role = AriaRole.presentation
      ariaHidden = true
      className = cn("[&>svg]:size-3.5", props.className)
      if (props.children != null) {
         +props.children
      } else {
         ChevronRightIcon {}
      }
   }
}

val BreadcrumbEllipsis = FC<PropsWithClassName>("BreadcrumbEllipsis") { props ->
   li {
      spread(props, ExcludeSets.CLASS_NAME_CHILDREN)
      dataSlot = "breadcrumb-ellipsis"
      role = AriaRole.presentation
      ariaHidden = true
      className = cn("flex size-9 items-center justify-center", props.className)
      MoreHorizontalIcon {}
      span {
         className = ClassName("sr-only")
         +"More"
      }
   }
}
