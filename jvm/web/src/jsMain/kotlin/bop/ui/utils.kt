package bop.ui

import js.objects.Object
import react.Props
import react.dom.html.HTMLAttributes
import web.html.HTMLElement

object ExcludeSets {
   val CLASS_NAME = setOf("className")
   val CLASS_NAME_CHILDREN = setOf("className", "children")
}

operator fun <E : HTMLElement> HTMLAttributes<E>.set(
   name: String, value: dynamic
) {
   asDynamic()[name] = value
}

operator fun <P : Props> P.set(name: String, value: dynamic) {
   asDynamic()[name] = value
}

inline var <P : Props> P.dataSlot: String?
   get() = asDynamic()["data-slot"] as String?
   set(value) {
      asDynamic()["data-slot"] = value
   }

inline var <P : Props> P.dataInset: Boolean?
   get() = if (asDynamic()["data-inset"] == null) null else asDynamic()["data-inset"].toString() == "true"
   set(value) {
      asDynamic()["data-inset"] = value?.toString()
   }

inline var <P : Props> P.dataVariant: String?
   get() = asDynamic()["data-variant"] as String?
   set(value) {
      asDynamic()["data-variant"] = value
   }

inline var <P : Props> P.dataActive: Boolean?
   get() = asDynamic()["data-active"] as Boolean?
   set(value) {
      asDynamic()["data-active"] = value
   }

inline var <P : Props> P.dataViewport: Boolean?
   get() = if (asDynamic()["data-viewport"] == null) null else asDynamic()["data-viewport"].toString() == "true"
   set(value) {
      asDynamic()["data-viewport"] = value?.toString()
   }

inline var <P : Props> P.dataSize: String?
   get() = asDynamic()["data-size"] as String?
   set(value) {
      asDynamic()["data-size"] = value
   }

fun <P0 : Any?, P1 : Any?> P0.spread(props: P1, exclude: String = ""): P0 {
   if (this == null || props == null) {
      return this
   }
   Object.keys(props).unsafeCast<Array<String>>().forEach { key ->
      if (key != exclude) {
         val value = props.asDynamic()[key]
         if (value != undefined) {
            asDynamic()[key] = value
         }
      }
   }
   return this
}

fun <P0 : Any?, P1 : Any?> P0.spread(props: P1, exclude: String, exclude2: String): P0 {
   if (this == null || props == null) {
      return this
   }
   Object.keys(props).unsafeCast<Array<String>>().forEach { key ->
      if (key != exclude && key != exclude2) {
         val value = props.asDynamic()[key]
         if (value != undefined) {
            asDynamic()[key] = value
         }
      }
   }
   return this
}

fun <P0 : Any?, P1 : Any?> P0.spread(props: P1, exclude: Set<String>): P0 {
   if (this == null || props == null) {
      return this
   }
   Object.keys(props).unsafeCast<Array<String>>().forEach { key ->
      if (key !in exclude) {
         val value = props.asDynamic()[key]
         if (value != undefined) {
            asDynamic()[key] = value
         }
      }
   }
   return this
}
