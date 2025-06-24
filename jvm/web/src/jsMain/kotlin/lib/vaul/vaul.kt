package lib.vaul

interface DrawerDirection {
   companion object {
      val top = "top".unsafeCast<DrawerDirection>()
      val left = "left".unsafeCast<DrawerDirection>()
      val bottom = "bottom".unsafeCast<DrawerDirection>()
      val right = "right".unsafeCast<DrawerDirection>()
   }
}