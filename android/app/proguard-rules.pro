# JNI: KuCore calls into these by name.
-keep class digital.kuduy.kudownloader.core.Native { *; }
# The browser's JavaScript bridge.
-keepclassmembers class digital.kuduy.kudownloader.browser.PageBridge {
    @android.webkit.JavascriptInterface <methods>;
}
# kotlinx.serialization
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.**
-keepclassmembers @kotlinx.serialization.Serializable class digital.kuduy.kudownloader.** {
    *** Companion;
    *** INSTANCE;
    kotlinx.serialization.KSerializer serializer(...);
}
-keep,includedescriptorclasses class digital.kuduy.kudownloader.**$$serializer { *; }
# youtubedl-android (uses Jackson reflection and commons-compress)
-keep class com.yausername.** { *; }
-keep class com.fasterxml.jackson.** { *; }
-dontwarn com.fasterxml.jackson.**
-dontwarn org.apache.commons.**
-dontwarn org.tukaani.xz.**
-dontwarn com.github.luben.zstd.**
-dontwarn org.brotli.dec.**
# youtubedl-android unpacks Python and FFmpeg with commons-compress / commons-io.
-keep class org.apache.commons.compress.** { *; }
-keep class org.apache.commons.io.** { *; }
