package com.argentum

import android.app.Application
import com.argentum.data.repository.SettingsRepository

class ArgentumApplication : Application() {
    
    lateinit var settingsRepository: SettingsRepository
        private set
    
    override fun onCreate() {
        super.onCreate()
        instance = this
        settingsRepository = SettingsRepository(applicationContext)
    }
    
    companion object {
        lateinit var instance: ArgentumApplication
            private set
    }
}
