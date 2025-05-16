package bop.ui

import lucide.MoreHorizontalIcon
import radix.ui.ChevronRightIcon
import radix.ui.Slot
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
      ariaLabel = "breadcrumb"
      this["data-slot"] = "breadcrumb"
      spread(props)
   }
}

val BreadcrumbList = FC<DefaultProps>("BreadcrumbList") { props ->
   li {
      this["data-slot"] = "breadcrumb-list"
      className = cn(
         "text-muted-foreground flex flex-wrap items-center gap-1.5 text-sm break-words sm:gap-2.5",
         props.className,
      )
      spread(props, "className")
   }
}

val BreadcrumbItem = FC<DefaultProps>("BreadcrumbItem") { props ->
   li {
      this["data-slot"] = "breadcrumb-item"
      className = cn("inline-flex items-center gap-1.5", props.className)
      spread(props, "className")
   }
}

val BreadcrumbLink = FC<AsChildProps>("BreadcrumbLink") { props ->
   val component = if (props.asChild == true) Slot else a
   component {
      this["data-slot"] = "breadcrumb-link"
      className = cn("hover:text-foreground transition-colors", props.className)
      spread(props, "className")
   }
}

val BreadcrumbSeparator = FC<AsChildProps>("BreadcrumbSeparator") { props ->
   li {
      this["data-slot"] = "breadcrumb-separator"
      role = AriaRole.presentation
      ariaHidden = true
      className = cn("[&>svg]:size-3.5", props.className)
      spread(props, ExcludeSets.CLASS_NAME_CHILDREN)
      if (props.children != null) {
         +props.children
      } else {
         ChevronRightIcon {}
      }
   }
}

val BreadcrumbEllipsis = FC<PropsWithClassName>("BreadcrumbEllipsis") { props ->
   li {
      this["data-slot"] = "breadcrumb-ellipsis"
      role = AriaRole.presentation
      ariaHidden = true
      className = cn("flex size-9 items-center justify-center", props.className)
      spread(props, ExcludeSets.CLASS_NAME_CHILDREN)
      MoreHorizontalIcon {}
      span {
         className = ClassName("sr-only")
         +"More"
      }
   }
}
