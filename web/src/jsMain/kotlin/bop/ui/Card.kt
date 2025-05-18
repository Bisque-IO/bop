package bop.ui

import react.FC
import react.dom.html.ReactHTML.div

val Card = FC<DefaultProps>("Card") { props ->
   div {
      spread(props, "className")
      dataSlot = "card"
      className = cn(
         "bg-card text-card-foreground flex flex-col gap-6 rounded-xl border py-6 shadow-sm",
         props.className,
      )
   }
}

val CardHeader = FC<DefaultProps>("CardHeader") { props ->
   div {
      spread(props, "className")
      dataSlot = "card-header"
      className = cn(
         "@container/card-header grid auto-rows-min grid-rows-[auto_auto] items-start gap-1.5 px-6 has-data-[slot=card-action]:grid-cols-[1fr_auto] [.border-b]:pb-6",
         props.className,
      )
   }
}

val CardTitle = FC<DefaultProps>("CardTitle") { props ->
   div {
      spread(props, "className")
      dataSlot = "card-title"
      className = cn("leading-none font-semibold", props.className)
   }
}

val CardDescription = FC<DefaultProps>("CardDescription") { props ->
   div {
      spread(props, "className")
      dataSlot = "card-description"
      className = cn("text-muted-foreground text-sm", props.className)
   }
}

val CardAction = FC<DefaultProps>("CardAction") { props ->
   div {
      spread(props, "className")
      dataSlot = "card-action"
      className = cn("col-start-2 row-span-2 row-start-1 self-start justify-self-end", props.className)
   }
}

val CardContent = FC<DefaultProps>("CardContent") { props ->
   div {
      spread(props, "className")
      dataSlot = "card-content"
      className = cn("px-6", props.className)
   }
}

val CardFooter = FC<DefaultProps>("CardFooter") { props ->
   div {
      spread(props, "className")
      dataSlot = "card-footer"
      className = cn("flex items-center px-6 [.border-t]:pt-6", props.className)
   }
}
