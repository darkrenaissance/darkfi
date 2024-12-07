//% IMPORTS

import android.view.inputmethod.InputMethodManager;

import autosuggest.CustomInputConnection;

//% END

//% MAIN_ACTIVITY_BODY

public void cancelComposition() {
    InputMethodManager imm =
        (InputMethodManager)getSystemService(Context.INPUT_METHOD_SERVICE);
    imm.restartInput(view);
}

//% END

//% QUAD_SURFACE_ON_CREATE_INPUT_CONNECTION

// Needed to fix error: unreachable statement in Java
if (true) {
    outAttrs.inputType = EditorInfo.TYPE_CLASS_TEXT
        | EditorInfo.TYPE_TEXT_FLAG_AUTO_CORRECT;
    outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN
        | EditorInfo.IME_ACTION_NONE;
    return new CustomInputConnection(this, outAttrs);
}

//% END

