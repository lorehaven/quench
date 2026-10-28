<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-permission android:name="android.permission.INTERNET" />
    <!-- quench:permissions -->

    <application
        android:label="{{ name }}"
        android:theme="@style/AppTheme"
        android:allowBackup="false"
        android:supportsRtl="true"
        android:extractNativeLibs="true"
        android:enableOnBackInvokedCallback="false"
        android:networkSecurityConfig="@xml/network_security_config">
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:launchMode="singleTop"
            android:windowSoftInputMode="adjustResize"
            android:configChanges="orientation|screenSize|smallestScreenSize|screenLayout|keyboard|keyboardHidden|uiMode|density|fontScale|layoutDirection|locale">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
            <!-- quench:activity -->
        </activity>
        <!-- quench:components -->
    </application>
</manifest>
