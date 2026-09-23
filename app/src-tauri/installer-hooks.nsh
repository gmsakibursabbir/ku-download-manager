; KuDownloader NSIS hooks. The app registers its browser connection (per user,
; HKCU) on first start; uninstall removes it again.

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.kuduy.kudownloader"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\com.kuduy.kudownloader"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.kuduy.kudownloader"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.kuduy.kudownloader"
  DeleteRegKey HKCU "Software\Vivaldi\NativeMessagingHosts\com.kuduy.kudownloader"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\com.kuduy.kudownloader"
!macroend
